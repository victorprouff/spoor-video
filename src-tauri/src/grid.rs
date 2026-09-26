//! La vue grille (§5) : des tuiles de séquences, filtrables, pour le tri grossier.

use rusqlite::{Connection, ToSql};

use crate::db::DbError;
use crate::traps::EFFECTIVE_RECORDED_AT;

#[derive(Debug, Default, serde::Deserialize)]
pub struct GridFilter {
    /// Une seule séquence : celle qu'on rouvre depuis l'onglet Vidéos pour la corriger.
    pub sequence_id: Option<String>,
    pub trap_id: Option<String>,
    /// `unreviewed` | `reviewed` | `all` (défaut : `all`).
    pub review: Option<String>,
    /// États retenus, en **OU**.
    #[serde(default)]
    pub states: Vec<String>,
    /// Espèces retenues, en **OU** — comme dans Spoor.
    #[serde(default)]
    pub species: Vec<String>,
    /// Confiance minimale retenue : `certain` ne garde que le certain, `possible`
    /// garde tout. Une séquence passe si **au moins une** de ses espèces l'atteint.
    pub confidence_min: Option<String>,
    /// Bornes de date, en `YYYY-MM-DD` (incluses).
    pub from: Option<String>,
    pub to: Option<String>,
    /// Mois retenus, **toutes années confondues** — le cœur de l'analyse long terme :
    /// comparer les mois de décembre entre eux, et non un hiver donné.
    #[serde(default)]
    pub months: Vec<i64>,
    /// Plage horaire, en heures (0–23). `from > to` traverse minuit : 22→4 est la nuit.
    pub hour_from: Option<i64>,
    pub hour_to: Option<i64>,
    /// Position par rapport au soleil, en **OU** : day / dawn / dusk / night.
    #[serde(default)]
    pub sun_phases: Vec<String>,
    /// Durée du passage, en secondes.
    pub duration_min_s: Option<i64>,
    pub duration_max_s: Option<i64>,
    /// Texte libre, cherché dans le nom de fichier des vidéos de la séquence. Les notes
    /// de séquence n'en font plus partie : aucun écran ne permet de les saisir.
    pub query: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Les confiances, de la plus forte à la plus faible. Demander « probable » retient
/// donc aussi le certain — un seuil, pas une égalité.
pub fn confidence_at_least(level: &str) -> Vec<&'static str> {
    match level {
        "certain" => vec!["certain"],
        "probable" => vec!["certain", "probable"],
        _ => vec!["certain", "probable", "possible"],
    }
}

#[derive(Debug, serde::Serialize)]
pub struct GridTile {
    pub id: String,
    pub trap_id: String,
    pub trap_name: String,
    pub started_at: String,
    pub ended_at: String,
    pub video_count: i64,
    pub duration_s: i64,
    pub state: Option<String>,
    pub notes: Option<String>,
    pub reviewed: bool,
    pub auto_grouped: bool,
    /// Vignettes des vidéos de la séquence, dans l'ordre. Survoler la tuile les fait
    /// défiler : une planche de contact sans générer d'image supplémentaire.
    pub thumbnails: Vec<String>,
    pub species: Vec<TileSpecies>,
    /// Vidéos dont le fichier n'est plus lisible (`purged` ou `missing`).
    pub unplayable_count: i64,
    /// day / dawn / dusk / night, ou `None` si le piège n'a pas de position.
    pub sun_phase: Option<String>,
    pub minutes_from_sunset: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct TileSpecies {
    pub id: String,
    pub common_name: String,
    pub color: Option<String>,
    pub confidence: String,
}

#[derive(Debug, serde::Serialize)]
pub struct GridPage {
    pub tiles: Vec<GridTile>,
    pub total: i64,
    /// Nombre de séquences non dépouillées, tous filtres de dépouillement mis à part :
    /// le reste-à-faire ne doit pas disparaître parce qu'on a filtré.
    pub unreviewed_total: i64,
}

/// Construit la clause `WHERE` et ses paramètres. Partagée avec les statistiques :
/// analyser une sélection doit donner exactement ce que la grille montre.
pub fn where_clause(filter: &GridFilter) -> (String, Vec<Box<dyn ToSql>>) {
    let mut clauses = vec!["s.deleted_at IS NULL".to_string()];
    let mut params: Vec<Box<dyn ToSql>> = Vec::new();

    if let Some(sequence_id) = &filter.sequence_id {
        clauses.push("s.id = ?".into());
        params.push(Box::new(sequence_id.clone()));
    }

    if let Some(trap_id) = &filter.trap_id {
        clauses.push("s.trap_id = ?".into());
        params.push(Box::new(trap_id.clone()));
    }

    match filter.review.as_deref() {
        Some("unreviewed") => clauses.push("s.reviewed_at IS NULL".into()),
        Some("reviewed") => clauses.push("s.reviewed_at IS NOT NULL".into()),
        _ => {}
    }

    if !filter.states.is_empty() {
        let holes = vec!["?"; filter.states.len()].join(",");
        clauses.push(format!("s.state IN ({holes})"));
        for state in &filter.states {
            params.push(Box::new(state.clone()));
        }
    }

    // Espèces : filtre en OU.
    if !filter.species.is_empty() {
        let holes = vec!["?"; filter.species.len()].join(",");
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM sequence_species ss
                     WHERE ss.sequence_id = s.id AND ss.species_id IN ({holes}))"
        ));
        for id in &filter.species {
            params.push(Box::new(id.clone()));
        }
    }

    if let Some(level) = &filter.confidence_min {
        let levels = confidence_at_least(level);
        let holes = vec!["?"; levels.len()].join(",");
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM sequence_species ss
                     WHERE ss.sequence_id = s.id AND ss.confidence IN ({holes}))"
        ));
        for l in levels {
            params.push(Box::new(l.to_string()));
        }
    }

    if let Some(from) = &filter.from {
        clauses.push("date(s.started_at) >= date(?)".into());
        params.push(Box::new(from.clone()));
    }
    if let Some(to) = &filter.to {
        clauses.push("date(s.started_at) <= date(?)".into());
        params.push(Box::new(to.clone()));
    }

    if !filter.months.is_empty() {
        let holes = vec!["?"; filter.months.len()].join(",");
        clauses.push(format!(
            "CAST(strftime('%m', s.started_at) AS INTEGER) IN ({holes})"
        ));
        for m in &filter.months {
            params.push(Box::new(*m));
        }
    }

    // Plage horaire. Une plage qui traverse minuit (22 → 4) est un OU, pas un ET :
    // sans ce cas, filtrer « la nuit » ne rendrait jamais rien.
    match (filter.hour_from, filter.hour_to) {
        (Some(a), Some(b)) if a <= b => {
            clauses.push("CAST(strftime('%H', s.started_at) AS INTEGER) BETWEEN ? AND ?".into());
            params.push(Box::new(a));
            params.push(Box::new(b));
        }
        (Some(a), Some(b)) => {
            clauses.push(
                "(CAST(strftime('%H', s.started_at) AS INTEGER) >= ?
                  OR CAST(strftime('%H', s.started_at) AS INTEGER) <= ?)"
                    .into(),
            );
            params.push(Box::new(a));
            params.push(Box::new(b));
        }
        _ => {}
    }

    if !filter.sun_phases.is_empty() {
        let holes = vec!["?"; filter.sun_phases.len()].join(",");
        clauses.push(format!("s.sun_phase IN ({holes})"));
        for p in &filter.sun_phases {
            params.push(Box::new(p.clone()));
        }
    }

    // Durée du passage : c'est elle qui distingue un animal qui traverse d'un animal
    // qui stationne. SQLite compte en secondes via le julien.
    let duration_sql = "CAST((julianday(s.ended_at) - julianday(s.started_at)) * 86400 AS INTEGER)";
    if let Some(min) = filter.duration_min_s {
        clauses.push(format!("{duration_sql} >= ?"));
        params.push(Box::new(min));
    }
    if let Some(max) = filter.duration_max_s {
        clauses.push(format!("{duration_sql} <= ?"));
        params.push(Box::new(max));
    }

    if let Some(q) = filter.query.as_ref().map(|q| q.trim()).filter(|q| !q.is_empty()) {
        let like = format!("%{q}%");
        clauses.push(
            "EXISTS (SELECT 1 FROM videos v WHERE v.sequence_id = s.id
                     AND v.deleted_at IS NULL AND v.file_name LIKE ? ESCAPE '\\')"
                .into(),
        );
        params.push(Box::new(like));
    }

    (clauses.join(" AND "), params)
}

pub fn page(conn: &Connection, filter: GridFilter) -> Result<GridPage, DbError> {
    let (where_sql, params) = where_clause(&filter);
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM sequences s WHERE {where_sql}"),
        refs.as_slice(),
        |r| r.get(0),
    )?;

    let unreviewed_total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sequences s WHERE s.deleted_at IS NULL AND s.reviewed_at IS NULL",
        [],
        |r| r.get(0),
    )?;

    let limit = filter.limit.unwrap_or(200).clamp(1, 1000);
    let offset = filter.offset.unwrap_or(0).max(0);

    let sql = format!(
        "SELECT s.id, s.trap_id, t.name, s.started_at, s.ended_at, s.video_count,
                s.state, s.notes, s.reviewed_at, s.auto_grouped,
                s.sun_phase, s.minutes_from_sunset
         FROM sequences s JOIN traps t ON t.id = s.trap_id
         WHERE {where_sql}
         ORDER BY s.started_at DESC
         LIMIT {limit} OFFSET {offset}"
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), |r| {
        let started: String = r.get(3)?;
        let ended: String = r.get(4)?;
        Ok(GridTile {
            id: r.get(0)?,
            trap_id: r.get(1)?,
            trap_name: r.get(2)?,
            duration_s: duration_between(&started, &ended),
            started_at: started,
            ended_at: ended,
            video_count: r.get(5)?,
            state: r.get(6)?,
            notes: r.get(7)?,
            reviewed: r.get::<_, Option<String>>(8)?.is_some(),
            auto_grouped: r.get::<_, i64>(9)? != 0,
            thumbnails: Vec::new(),
            species: Vec::new(),
            unplayable_count: 0,
            sun_phase: r.get(10)?,
            minutes_from_sunset: r.get(11)?,
        })
    })?;
    let mut tiles: Vec<GridTile> = rows.collect::<Result<_, _>>()?;

    // Les détails sont chargés tuile par tuile. Sur 200 tuiles c'est 600 petites
    // requêtes sur une base locale — quelques millisecondes, et un code qu'on relit.
    for tile in &mut tiles {
        tile.thumbnails = thumbnails_of(conn, &tile.id)?;
        tile.species = species_of(conn, &tile.id)?;
        tile.unplayable_count = conn.query_row(
            "SELECT COUNT(*) FROM videos
             WHERE sequence_id = ?1 AND deleted_at IS NULL AND file_state <> 'present'",
            [&tile.id],
            |r| r.get(0),
        )?;
    }

    Ok(GridPage {
        tiles,
        total,
        unreviewed_total,
    })
}

fn duration_between(started: &str, ended: &str) -> i64 {
    match (
        crate::media::parse_timestamp(started),
        crate::media::parse_timestamp(ended),
    ) {
        (Some(a), Some(b)) => (b - a).num_seconds().max(0),
        _ => 0,
    }
}

fn thumbnails_of(conn: &Connection, sequence_id: &str) -> Result<Vec<String>, DbError> {
    let sql = format!(
        "SELECT v.thumbnail_path FROM videos v JOIN traps t ON t.id = v.trap_id
         WHERE v.sequence_id = ?1 AND v.deleted_at IS NULL AND v.thumbnail_path IS NOT NULL
         ORDER BY {eff}, v.file_name
         LIMIT 12",
        eff = EFFECTIVE_RECORDED_AT
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([sequence_id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn species_of(conn: &Connection, sequence_id: &str) -> Result<Vec<TileSpecies>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT sp.id, sp.common_name, sp.color, ss.confidence
         FROM sequence_species ss JOIN species sp ON sp.id = ss.species_id
         WHERE ss.sequence_id = ?1
         ORDER BY sp.sort_order, sp.common_name",
    )?;
    let rows = stmt.query_map([sequence_id], |r| {
        Ok(TileSpecies {
            id: r.get(0)?,
            common_name: r.get(1)?,
            color: r.get(2)?,
            confidence: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::annotations::{annotate, Annotation, SpeciesPick};
    use crate::db::migrations;

    pub fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO traps (id, name, created_at, updated_at)
             VALUES ('t1', 'Mare basse', datetime('now'), datetime('now')),
                    ('t2', 'Chablis nord', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        for (id, trap) in [("s1", "t1"), ("s2", "t1"), ("s3", "t2")] {
            conn.execute(
                "INSERT INTO sequences (id, trap_id, started_at, ended_at, video_count,
                     auto_grouped, created_at, updated_at)
                 VALUES (?1, ?2, '2026-03-14T21:00:00Z', '2026-03-14T21:05:00Z', 2, 1,
                         datetime('now'), datetime('now'))",
                [id, trap],
            )
            .unwrap();
        }
        conn
    }

    /// Repositionne une séquence dans le temps, avec une durée donnée.
    pub fn move_to(conn: &Connection, id: &str, started: &str, duration_s: i64) {
        conn.execute(
            "UPDATE sequences SET started_at = ?2,
                 ended_at = datetime(?2, '+' || ?3 || ' seconds') WHERE id = ?1",
            rusqlite::params![id, started, duration_s],
        )
        .unwrap();
    }

    fn filter() -> GridFilter {
        GridFilter::default()
    }

    fn pick(conn: &Connection, n: usize) -> String {
        let mut stmt = conn.prepare("SELECT id FROM species LIMIT ?1").unwrap();
        let ids: Vec<String> = stmt
            .query_map([n + 1], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        ids[n].clone()
    }

    #[test]
    fn liste_toutes_les_sequences() {
        let conn = db();
        let page = page(&conn, filter()).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.tiles.len(), 3);
        assert_eq!(page.unreviewed_total, 3);
        assert_eq!(page.tiles[0].duration_s, 300);
    }

    #[test]
    fn filtre_sur_une_seule_sequence() {
        let conn = db();
        let first = page(&conn, filter()).unwrap().tiles[1].id.clone();
        let page = page(
            &conn,
            GridFilter {
                sequence_id: Some(first.clone()),
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.tiles[0].id, first);
    }

    #[test]
    fn filtre_par_piege() {
        let conn = db();
        let page = page(
            &conn,
            GridFilter {
                trap_id: Some("t1".into()),
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(page.total, 2);
    }

    #[test]
    fn filtre_le_reste_a_faire() {
        let mut conn = db();
        annotate(
            &mut conn,
            &["s1".to_string()],
            Annotation {
                state: Some("empty".into()),
                ..Default::default()
            },
        )
        .unwrap();

        let a_faire = page(
            &conn,
            GridFilter {
                review: Some("unreviewed".into()),
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(a_faire.total, 2);

        let faites = page(
            &conn,
            GridFilter {
                review: Some("reviewed".into()),
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(faites.total, 1);
        assert!(faites.tiles[0].reviewed);
        assert_eq!(
            faites.unreviewed_total, 2,
            "le reste-à-faire ne disparaît pas parce qu'on a filtré"
        );
    }

    #[test]
    fn les_especes_filtrent_en_ou() {
        let mut conn = db();
        let (a, b) = (pick(&conn, 0), pick(&conn, 1));
        annotate(
            &mut conn,
            &["s1".to_string()],
            Annotation {
                add_species: vec![SpeciesPick {
                    species_id: a.clone(),
                    confidence: "certain".into(),
                    count_min: None,
                    count_max: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();
        annotate(
            &mut conn,
            &["s2".to_string()],
            Annotation {
                add_species: vec![SpeciesPick {
                    species_id: b.clone(),
                    confidence: "certain".into(),
                    count_min: None,
                    count_max: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();

        let page = page(
            &conn,
            GridFilter {
                species: vec![a, b],
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(page.total, 2, "deux espèces demandées : l'une OU l'autre");
    }

    #[test]
    fn la_tuile_porte_ses_especes() {
        let mut conn = db();
        let a = pick(&conn, 0);
        annotate(
            &mut conn,
            &["s1".to_string()],
            Annotation {
                add_species: vec![SpeciesPick {
                    species_id: a,
                    confidence: "probable".into(),
                    count_min: None,
                    count_max: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();

        let page = page(
            &conn,
            GridFilter {
                review: Some("reviewed".into()),
                ..filter()
            },
        )
        .unwrap();
        let tile = &page.tiles[0];
        assert_eq!(tile.species.len(), 1);
        assert_eq!(tile.species[0].confidence, "probable");
    }
}

#[cfg(test)]
mod filter_tests {
    use super::tests::*;
    use super::*;
    use crate::annotations::{annotate, Annotation, SpeciesPick};

    fn seq_ids(page: &GridPage) -> Vec<String> {
        let mut v: Vec<String> = page.tiles.iter().map(|t| t.id.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn filtre_par_plage_de_dates() {
        let conn = db();
        move_to(&conn, "s1", "2025-06-10T12:00:00", 60);
        move_to(&conn, "s2", "2026-03-14T21:00:00", 60);
        move_to(&conn, "s3", "2026-12-02T03:00:00", 60);

        let p = page(
            &conn,
            GridFilter {
                from: Some("2026-01-01".into()),
                to: Some("2026-06-30".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(seq_ids(&p), vec!["s2"]);
    }

    #[test]
    fn filtre_par_mois_toutes_annees_confondues() {
        let conn = db();
        // Deux décembres d'années différentes, et un juin.
        move_to(&conn, "s1", "2024-12-20T22:00:00", 60);
        move_to(&conn, "s2", "2026-12-02T03:00:00", 60);
        move_to(&conn, "s3", "2025-06-10T12:00:00", 60);

        let p = page(
            &conn,
            GridFilter {
                months: vec![12],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            seq_ids(&p),
            vec!["s1", "s2"],
            "décembre 2024 et décembre 2026 se comparent entre eux"
        );
    }

    #[test]
    fn une_plage_horaire_qui_traverse_minuit_est_un_ou() {
        let conn = db();
        move_to(&conn, "s1", "2026-03-14T23:30:00", 60);
        move_to(&conn, "s2", "2026-03-14T02:30:00", 60);
        move_to(&conn, "s3", "2026-03-14T13:00:00", 60);

        let nuit = page(
            &conn,
            GridFilter {
                hour_from: Some(22),
                hour_to: Some(4),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            seq_ids(&nuit),
            vec!["s1", "s2"],
            "22 h → 4 h doit retenir 23 h 30 et 2 h 30, pas rien"
        );

        let jour = page(
            &conn,
            GridFilter {
                hour_from: Some(9),
                hour_to: Some(17),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(seq_ids(&jour), vec!["s3"]);
    }

    #[test]
    fn la_confiance_minimale_est_un_seuil_pas_une_egalite() {
        let mut conn = db();
        let mut stmt = conn.prepare("SELECT id FROM species LIMIT 3").unwrap();
        let sp: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        drop(stmt);

        for (seq, conf, i) in [("s1", "certain", 0), ("s2", "probable", 1), ("s3", "possible", 2)] {
            annotate(
                &mut conn,
                &[seq.to_string()],
                Annotation {
                    add_species: vec![SpeciesPick {
                        species_id: sp[i].clone(),
                        confidence: conf.into(),
                        count_min: None,
                        count_max: None,
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
        }

        let p = page(
            &conn,
            GridFilter {
                confidence_min: Some("probable".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            seq_ids(&p),
            vec!["s1", "s2"],
            "« probable » retient aussi le certain"
        );

        let strict = page(
            &conn,
            GridFilter {
                confidence_min: Some("certain".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(seq_ids(&strict), vec!["s1"]);
    }

    #[test]
    fn filtre_par_duree_de_passage() {
        let conn = db();
        move_to(&conn, "s1", "2026-03-14T21:00:00", 30);
        move_to(&conn, "s2", "2026-03-14T22:00:00", 7200);
        move_to(&conn, "s3", "2026-03-14T23:00:00", 300);

        let longues = page(
            &conn,
            GridFilter {
                duration_min_s: Some(3600),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            seq_ids(&longues),
            vec!["s2"],
            "seule l'activité continue de deux heures est retenue"
        );
    }

    #[test]
    fn cherche_dans_les_noms_de_fichier_pas_dans_les_notes() {
        let mut conn = db();
        annotate(
            &mut conn,
            &["s1".to_string()],
            Annotation {
                notes: Some("laie suitée avec quatre marcassins".into()),
                ..Default::default()
            },
        )
        .unwrap();
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id, sequence_id,
                 imported_at, created_at, updated_at)
             VALUES ('v1', '/a', 'IMG_4242.mp4', 'h1', 't1', 's2',
                     datetime('now'), datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        let notes = page(
            &conn,
            GridFilter {
                query: Some("marcassin".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(seq_ids(&notes).is_empty(), "les notes ne sont plus cherchées");

        let fichier = page(
            &conn,
            GridFilter {
                query: Some("4242".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(seq_ids(&fichier), vec!["s2"]);
    }

    #[test]
    fn filtre_par_position_solaire() {
        let conn = db();
        conn.execute("UPDATE sequences SET sun_phase = 'night' WHERE id IN ('s1','s2')", [])
            .unwrap();
        conn.execute("UPDATE sequences SET sun_phase = 'day' WHERE id = 's3'", [])
            .unwrap();

        let p = page(
            &conn,
            GridFilter {
                sun_phases: vec!["night".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(seq_ids(&p), vec!["s1", "s2"]);
    }

    #[test]
    fn les_filtres_se_combinent() {
        let conn = db();
        move_to(&conn, "s1", "2026-12-20T22:00:00", 60);
        move_to(&conn, "s2", "2026-12-21T13:00:00", 60);
        move_to(&conn, "s3", "2026-06-10T22:00:00", 60);

        let p = page(
            &conn,
            GridFilter {
                months: vec![12],
                hour_from: Some(20),
                hour_to: Some(23),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(seq_ids(&p), vec!["s1"], "décembre ET le soir");
    }

    #[test]
    fn la_pagination_ne_ment_pas_sur_le_total() {
        let conn = db();
        let p = page(
            &conn,
            GridFilter {
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.tiles.len(), 2);
        assert_eq!(p.total, 3, "le total compte tout, pas seulement la page");
    }
}

//! La vue vidéo : une ligne par fichier, filtrable.
//!
//! Tout le reste de l'application raisonne en **séquences**, ce qui est la bonne unité
//! pour dépouiller et pour compter. Mais certaines questions se posent au fichier :
//! retrouver une capture précise, voir ce qui n'a pas de date, vérifier ce qui est
//! devenu illisible. D'où cette vue, et son propre jeu de critères.
//!
//! Différence qui compte : ici, **la date filtrée est celle de la vidéo**, pas celle du
//! début de sa séquence. Chercher « entre 2 h et 3 h » doit rendre les déclenchements de
//! cette tranche, pas les passages qui ont commencé avant minuit.

use rusqlite::{Connection, ToSql};

use crate::db::DbError;
use crate::grid::{confidence_at_least, GridFilter};
use crate::traps::EFFECTIVE_RECORDED_AT;

#[derive(Debug, Default, serde::Deserialize)]
pub struct VideoFilter {
    #[serde(flatten)]
    pub base: GridFilter,
    /// `present` | `purged` | `missing`, en **OU**.
    #[serde(default)]
    pub file_states: Vec<String>,
    /// `true` ne garde que les vidéos sans date exploitable — celles qui attendent
    /// une saisie manuelle et qu'aucune séquence ne porte.
    pub undated: Option<bool>,
    /// `true` ne garde que les vidéos sans position.
    pub unpositioned: Option<bool>,
    /// `date` (défaut, du plus récent au plus ancien) ou `date_asc`.
    pub sort: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct VideoRow {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_state: String,
    pub trap_id: String,
    pub trap_name: String,
    pub sequence_id: Option<String>,
    pub recorded_at: Option<String>,
    /// Vrai quand la date a été corrigée à la main.
    pub date_manual: bool,
    pub duration_s: Option<f64>,
    pub file_size: Option<i64>,
    pub thumbnail_path: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub position_manual: bool,
    pub sun_phase: Option<String>,
    pub state: Option<String>,
    pub reviewed: bool,
    pub species: Vec<String>,
    pub species_colors: Vec<Option<String>>,
    /// Rang de la vidéo dans sa séquence, et taille de celle-ci : « 2 / 5 ».
    pub sequence_size: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct VideoPage {
    pub rows: Vec<VideoRow>,
    pub total: i64,
    /// Durée cumulée de la sélection, en secondes.
    pub total_duration_s: f64,
    pub total_bytes: i64,
    pub undated_total: i64,
    pub unplayable_total: i64,
}

/// Clause `WHERE` au niveau du fichier.
///
/// Les critères qui portent sur l'identification (espèces, tags, état, dépouillement)
/// passent par la séquence de la vidéo ; les critères de temps portent sur la vidéo.
fn where_clause(filter: &VideoFilter) -> (String, Vec<Box<dyn ToSql>>) {
    let f = &filter.base;
    let eff = EFFECTIVE_RECORDED_AT;
    let mut clauses = vec!["v.deleted_at IS NULL".to_string()];
    let mut params: Vec<Box<dyn ToSql>> = Vec::new();

    if let Some(trap_id) = &f.trap_id {
        clauses.push("v.trap_id = ?".into());
        params.push(Box::new(trap_id.clone()));
    }

    match f.review.as_deref() {
        // Une vidéo hors séquence n'a jamais été dépouillée : elle compte dans le reste
        // à faire, au lieu de disparaître entre deux filtres.
        Some("unreviewed") => clauses.push("(s.reviewed_at IS NULL OR s.id IS NULL)".into()),
        Some("reviewed") => clauses.push("s.reviewed_at IS NOT NULL".into()),
        _ => {}
    }

    if !f.states.is_empty() {
        let holes = vec!["?"; f.states.len()].join(",");
        clauses.push(format!("s.state IN ({holes})"));
        for state in &f.states {
            params.push(Box::new(state.clone()));
        }
    }

    if !f.species.is_empty() {
        let holes = vec!["?"; f.species.len()].join(",");
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM sequence_species ss
                     WHERE ss.sequence_id = v.sequence_id AND ss.species_id IN ({holes}))"
        ));
        for id in &f.species {
            params.push(Box::new(id.clone()));
        }
    }

    if let Some(level) = &f.confidence_min {
        let levels = confidence_at_least(level);
        let holes = vec!["?"; levels.len()].join(",");
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM sequence_species ss
                     WHERE ss.sequence_id = v.sequence_id AND ss.confidence IN ({holes}))"
        ));
        for l in levels {
            params.push(Box::new(l.to_string()));
        }
    }

    // --- Temps : sur la vidéo elle-même -------------------------------------
    if let Some(from) = &f.from {
        clauses.push(format!("date({eff}) >= date(?)"));
        params.push(Box::new(from.clone()));
    }
    if let Some(to) = &f.to {
        clauses.push(format!("date({eff}) <= date(?)"));
        params.push(Box::new(to.clone()));
    }
    if !f.months.is_empty() {
        let holes = vec!["?"; f.months.len()].join(",");
        clauses.push(format!(
            "CAST(strftime('%m', {eff}) AS INTEGER) IN ({holes})"
        ));
        for m in &f.months {
            params.push(Box::new(*m));
        }
    }
    // Une plage qui traverse minuit (22 → 4) est un OU, sinon filtrer « la nuit »
    // ne rendrait jamais rien.
    match (f.hour_from, f.hour_to) {
        (Some(a), Some(b)) if a <= b => {
            clauses.push(format!(
                "CAST(strftime('%H', {eff}) AS INTEGER) BETWEEN ? AND ?"
            ));
            params.push(Box::new(a));
            params.push(Box::new(b));
        }
        (Some(a), Some(b)) => {
            clauses.push(format!(
                "(CAST(strftime('%H', {eff}) AS INTEGER) >= ?
                  OR CAST(strftime('%H', {eff}) AS INTEGER) <= ?)"
            ));
            params.push(Box::new(a));
            params.push(Box::new(b));
        }
        _ => {}
    }

    if !f.sun_phases.is_empty() {
        let holes = vec!["?"; f.sun_phases.len()].join(",");
        clauses.push(format!("s.sun_phase IN ({holes})"));
        for p in &f.sun_phases {
            params.push(Box::new(p.clone()));
        }
    }

    // Durée : celle de la vidéo, pas celle du passage.
    if let Some(min) = f.duration_min_s {
        clauses.push("v.duration_s >= ?".into());
        params.push(Box::new(min));
    }
    if let Some(max) = f.duration_max_s {
        clauses.push("v.duration_s <= ?".into());
        params.push(Box::new(max));
    }

    if let Some(q) = f.query.as_ref().map(|q| q.trim()).filter(|q| !q.is_empty()) {
        clauses.push("v.file_name LIKE ?".into());
        params.push(Box::new(format!("%{q}%")));
    }

    // --- Critères propres au fichier ----------------------------------------
    if !filter.file_states.is_empty() {
        let holes = vec!["?"; filter.file_states.len()].join(",");
        clauses.push(format!("v.file_state IN ({holes})"));
        for state in &filter.file_states {
            params.push(Box::new(state.clone()));
        }
    }
    if filter.undated == Some(true) {
        clauses.push(format!("{eff} IS NULL"));
    }
    if filter.unpositioned == Some(true) {
        clauses.push("v.latitude IS NULL".into());
    }

    (clauses.join(" AND "), params)
}

const FROM: &str = "FROM videos v
     JOIN traps t ON t.id = v.trap_id
     LEFT JOIN sequences s ON s.id = v.sequence_id AND s.deleted_at IS NULL";

pub fn page(conn: &Connection, filter: VideoFilter) -> Result<VideoPage, DbError> {
    let eff = EFFECTIVE_RECORDED_AT;
    let (where_sql, params) = where_clause(&filter);
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let p = refs.as_slice();

    let (total, total_duration_s, total_bytes): (i64, f64, i64) = conn.query_row(
        &format!(
            "SELECT COUNT(*), COALESCE(SUM(v.duration_s), 0), COALESCE(SUM(v.file_size), 0)
             {FROM} WHERE {where_sql}"
        ),
        p,
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    // Les deux chiffres qui doivent rester visibles quels que soient les filtres :
    // ce qui attend une date, et ce qui n'est plus lisible.
    let undated_total: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM videos v JOIN traps t ON t.id = v.trap_id
             WHERE v.deleted_at IS NULL AND {eff} IS NULL"
        ),
        [],
        |r| r.get(0),
    )?;
    let unplayable_total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM videos WHERE deleted_at IS NULL AND file_state <> 'present'",
        [],
        |r| r.get(0),
    )?;

    let limit = filter.base.limit.unwrap_or(200).clamp(1, 2000);
    let offset = filter.base.offset.unwrap_or(0).max(0);
    let direction = if filter.sort.as_deref() == Some("date_asc") {
        "ASC"
    } else {
        "DESC"
    };

    let sql = format!(
        "SELECT v.id, v.file_name, v.file_path, v.file_state, v.trap_id, t.name,
                v.sequence_id, {eff}, v.recorded_at_manual IS NOT NULL,
                v.duration_s, v.file_size, v.thumbnail_path,
                v.latitude, v.longitude, v.position_manual,
                s.sun_phase, s.state, s.reviewed_at IS NOT NULL,
                COALESCE(s.video_count, 0)
         {FROM} WHERE {where_sql}
         ORDER BY {eff} {direction}, v.file_name
         LIMIT {limit} OFFSET {offset}"
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(p, |r| {
        Ok(VideoRow {
            id: r.get(0)?,
            file_name: r.get(1)?,
            file_path: r.get(2)?,
            file_state: r.get(3)?,
            trap_id: r.get(4)?,
            trap_name: r.get(5)?,
            sequence_id: r.get(6)?,
            recorded_at: r.get(7)?,
            date_manual: r.get::<_, i64>(8)? != 0,
            duration_s: r.get(9)?,
            file_size: r.get(10)?,
            thumbnail_path: r.get(11)?,
            latitude: r.get(12)?,
            longitude: r.get(13)?,
            position_manual: r.get::<_, i64>(14)? != 0,
            sun_phase: r.get(15)?,
            state: r.get(16)?,
            reviewed: r.get::<_, i64>(17)? != 0,
            sequence_size: r.get(18)?,
            species: Vec::new(),
            species_colors: Vec::new(),
        })
    })?;
    let mut rows: Vec<VideoRow> = rows.collect::<Result<_, _>>()?;

    // Espèces, une requête par ligne affichée : sur 200 lignes d'une base
    // locale c'est quelques millisecondes, et le code se relit.
    for row in &mut rows {
        let Some(sequence_id) = &row.sequence_id else {
            continue;
        };
        let mut stmt = conn.prepare(
            "SELECT sp.common_name, sp.color FROM sequence_species ss
             JOIN species sp ON sp.id = ss.species_id
             WHERE ss.sequence_id = ?1 ORDER BY sp.sort_order",
        )?;
        let pairs = stmt.query_map([sequence_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        for pair in pairs {
            let (name, color) = pair?;
            row.species.push(name);
            row.species_colors.push(color);
        }
    }

    Ok(VideoPage {
        rows,
        total,
        total_duration_s,
        total_bytes,
        undated_total,
        unplayable_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{annotate, Annotation, SpeciesPick};
    use crate::db::migrations;
    use rusqlite::params;

    fn db() -> Connection {
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
        conn.execute(
            "INSERT INTO sequences (id, trap_id, started_at, ended_at, video_count,
                 auto_grouped, created_at, updated_at)
             VALUES ('s1', 't1', '2026-03-14 21:00:00', '2026-03-14 21:05:00', 2, 1,
                     datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn
    }

    fn video(conn: &Connection, id: &str, trap: &str, at: Option<&str>, seq: Option<&str>) {
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id,
                 sequence_id, recorded_at, duration_s, file_size, file_state,
                 imported_at, created_at, updated_at)
             VALUES (?1, ?1, ?1, ?1, ?2, ?3, ?4, 12.0, 1000, 'present',
                     datetime('now'), datetime('now'), datetime('now'))",
            params![id, trap, seq, at],
        )
        .unwrap();
    }

    fn filter() -> VideoFilter {
        VideoFilter::default()
    }

    fn ids(page: &VideoPage) -> Vec<String> {
        let mut v: Vec<String> = page.rows.iter().map(|r| r.id.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn liste_toutes_les_videos() {
        let conn = db();
        video(&conn, "v1", "t1", Some("2026-03-14T21:00:00Z"), Some("s1"));
        video(&conn, "v2", "t1", Some("2026-03-14T21:02:00Z"), Some("s1"));
        video(&conn, "v3", "t2", Some("2026-03-15T02:00:00Z"), None);

        let p = page(&conn, filter()).unwrap();
        assert_eq!(p.total, 3);
        assert_eq!(p.total_duration_s, 36.0);
        assert_eq!(p.total_bytes, 3000);
    }

    #[test]
    fn une_video_hors_sequence_reste_visible() {
        let conn = db();
        video(&conn, "orpheline", "t1", None, None);
        let p = page(&conn, filter()).unwrap();
        assert_eq!(p.total, 1, "une vidéo sans date ne doit pas disparaître");
        assert_eq!(p.undated_total, 1);
    }

    #[test]
    fn filtre_sur_l_heure_de_la_video_pas_du_passage() {
        let conn = db();
        // Un passage qui commence à 23 h 55 et dont la seconde vidéo est à 0 h 05.
        conn.execute(
            "UPDATE sequences SET started_at = '2026-03-14 23:55:00' WHERE id = 's1'",
            [],
        )
        .unwrap();
        video(&conn, "avant", "t1", Some("2026-03-14T23:55:00Z"), Some("s1"));
        video(&conn, "apres", "t1", Some("2026-03-15T00:05:00Z"), Some("s1"));

        let p = page(
            &conn,
            VideoFilter {
                base: GridFilter {
                    hour_from: Some(0),
                    hour_to: Some(1),
                    ..Default::default()
                },
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(
            ids(&p),
            vec!["apres"],
            "chercher « entre 0 h et 1 h » doit rendre le déclenchement de cette tranche, \
             pas le passage qui a commencé avant minuit"
        );
    }

    #[test]
    fn filtre_par_espece_a_travers_la_sequence() {
        let mut conn = db();
        video(&conn, "v1", "t1", Some("2026-03-14T21:00:00Z"), Some("s1"));
        video(&conn, "v2", "t1", Some("2026-03-14T21:02:00Z"), Some("s1"));
        video(&conn, "v3", "t2", Some("2026-03-15T02:00:00Z"), None);

        let sp: String = conn
            .query_row("SELECT id FROM species LIMIT 1", [], |r| r.get(0))
            .unwrap();
        annotate(
            &mut conn,
            &["s1".to_string()],
            Annotation {
                add_species: vec![SpeciesPick {
                    species_id: sp.clone(),
                    confidence: "certain".into(),
                    count_min: None,
                    count_max: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();

        let p = page(
            &conn,
            VideoFilter {
                base: GridFilter {
                    species: vec![sp],
                    ..Default::default()
                },
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(
            ids(&p),
            vec!["v1", "v2"],
            "les deux vidéos du passage identifié, pas la troisième"
        );
        assert_eq!(p.rows[0].species.len(), 1, "l'espèce est portée par la ligne");
    }

    #[test]
    fn filtre_les_videos_sans_date() {
        let conn = db();
        video(&conn, "datee", "t1", Some("2026-03-14T21:00:00Z"), Some("s1"));
        video(&conn, "sans", "t1", None, None);

        let p = page(
            &conn,
            VideoFilter {
                undated: Some(true),
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(ids(&p), vec!["sans"]);
    }

    #[test]
    fn filtre_les_videos_devenues_illisibles() {
        let conn = db();
        video(&conn, "v1", "t1", Some("2026-03-14T21:00:00Z"), Some("s1"));
        video(&conn, "v2", "t1", Some("2026-03-14T21:02:00Z"), Some("s1"));
        conn.execute("UPDATE videos SET file_state = 'purged' WHERE id = 'v2'", [])
            .unwrap();

        let p = page(
            &conn,
            VideoFilter {
                file_states: vec!["purged".into(), "missing".into()],
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(ids(&p), vec!["v2"]);
        assert_eq!(p.unplayable_total, 1);
    }

    #[test]
    fn filtre_les_videos_sans_position() {
        let conn = db();
        video(&conn, "v1", "t1", Some("2026-03-14T21:00:00Z"), Some("s1"));
        video(&conn, "v2", "t1", Some("2026-03-14T21:02:00Z"), Some("s1"));
        conn.execute("UPDATE videos SET latitude = 47.3, longitude = 5.0 WHERE id = 'v1'", [])
            .unwrap();

        let p = page(
            &conn,
            VideoFilter {
                unpositioned: Some(true),
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(ids(&p), vec!["v2"]);
    }

    #[test]
    fn le_tri_par_date_va_dans_les_deux_sens() {
        let conn = db();
        video(&conn, "vieille", "t1", Some("2025-01-01T10:00:00Z"), None);
        video(&conn, "recente", "t1", Some("2026-06-01T10:00:00Z"), None);

        let recent = page(&conn, filter()).unwrap();
        assert_eq!(recent.rows[0].id, "recente", "le plus récent d'abord par défaut");

        let ancien = page(
            &conn,
            VideoFilter {
                sort: Some("date_asc".into()),
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(ancien.rows[0].id, "vieille");
    }

    #[test]
    fn une_video_hors_sequence_compte_comme_a_depouiller() {
        let conn = db();
        video(&conn, "orpheline", "t1", None, None);
        let p = page(
            &conn,
            VideoFilter {
                base: GridFilter {
                    review: Some("unreviewed".into()),
                    ..Default::default()
                },
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(
            p.total, 1,
            "elle n'a jamais été dépouillée : elle ne doit pas disparaître entre deux filtres"
        );
    }

    #[test]
    fn la_pagination_ne_ment_pas_sur_le_total() {
        let conn = db();
        for i in 0..5 {
            video(&conn, &format!("v{i}"), "t1", Some("2026-03-14T21:00:00Z"), None);
        }
        let p = page(
            &conn,
            VideoFilter {
                base: GridFilter {
                    limit: Some(2),
                    ..Default::default()
                },
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(p.rows.len(), 2);
        assert_eq!(p.total, 5);
    }
}

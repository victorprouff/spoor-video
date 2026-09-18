//! La vue grille (§5) : des tuiles de séquences, filtrables, pour le tri grossier.

use rusqlite::{Connection, ToSql};

use crate::db::DbError;
use crate::traps::EFFECTIVE_RECORDED_AT;

#[derive(Debug, Default, serde::Deserialize)]
pub struct GridFilter {
    pub trap_id: Option<String>,
    /// `unreviewed` | `reviewed` | `all` (défaut : `all`).
    pub review: Option<String>,
    /// États retenus, en **OU**.
    #[serde(default)]
    pub states: Vec<String>,
    /// Espèces retenues, en **OU** — comme dans Spoor.
    #[serde(default)]
    pub species: Vec<String>,
    /// Tags retenus, en **ET** : chaque tag ajouté affine.
    #[serde(default)]
    pub tags: Vec<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
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
    pub tags: Vec<String>,
    /// Vidéos dont le fichier n'est plus lisible (`purged` ou `missing`).
    pub unplayable_count: i64,
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

/// Construit la clause `WHERE` et ses paramètres.
fn where_clause(filter: &GridFilter) -> (String, Vec<Box<dyn ToSql>>) {
    let mut clauses = vec!["s.deleted_at IS NULL".to_string()];
    let mut params: Vec<Box<dyn ToSql>> = Vec::new();

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

    // Tags : filtre en ET — chaque tag ajouté affine, il n'élargit pas.
    for tag in &filter.tags {
        clauses.push(
            "EXISTS (SELECT 1 FROM sequence_tags st JOIN tags t2 ON t2.id = st.tag_id
                     WHERE st.sequence_id = s.id AND t2.name = ?)"
                .into(),
        );
        params.push(Box::new(tag.clone()));
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
                s.state, s.notes, s.reviewed_at, s.auto_grouped
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
            tags: Vec::new(),
            unplayable_count: 0,
        })
    })?;
    let mut tiles: Vec<GridTile> = rows.collect::<Result<_, _>>()?;

    // Les détails sont chargés tuile par tuile. Sur 200 tuiles c'est 600 petites
    // requêtes sur une base locale — quelques millisecondes, et un code qu'on relit.
    for tile in &mut tiles {
        tile.thumbnails = thumbnails_of(conn, &tile.id)?;
        tile.species = species_of(conn, &tile.id)?;
        tile.tags = tags_of(conn, &tile.id)?;
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

fn tags_of(conn: &Connection, sequence_id: &str) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM sequence_tags st JOIN tags t ON t.id = st.tag_id
         WHERE st.sequence_id = ?1 ORDER BY t.name",
    )?;
    let rows = stmt.query_map([sequence_id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{annotate, Annotation, SpeciesPick};
    use crate::db::migrations;

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
    fn les_tags_filtrent_en_et() {
        let mut conn = db();
        annotate(
            &mut conn,
            &["s1".to_string()],
            Annotation {
                add_tags: vec!["nuit".into(), "juvénile".into()],
                ..Default::default()
            },
        )
        .unwrap();
        annotate(
            &mut conn,
            &["s2".to_string()],
            Annotation {
                add_tags: vec!["nuit".into()],
                ..Default::default()
            },
        )
        .unwrap();

        let un = page(
            &conn,
            GridFilter {
                tags: vec!["nuit".into()],
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(un.total, 2);

        let deux = page(
            &conn,
            GridFilter {
                tags: vec!["nuit".into(), "juvénile".into()],
                ..filter()
            },
        )
        .unwrap();
        assert_eq!(deux.total, 1, "chaque tag ajouté affine, il n'élargit pas");
    }

    #[test]
    fn la_tuile_porte_ses_especes_et_ses_tags() {
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
                add_tags: vec!["nuit".into()],
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
        assert_eq!(tile.tags, vec!["nuit"]);
    }
}

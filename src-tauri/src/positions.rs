//! Position des vidéos (migration 005).
//!
//! Un piège peut être déplacé. Sa position est la **valeur par défaut** donnée aux
//! vidéos au moment de l'indexation ; ensuite chaque vidéo garde la sienne. Déplacer
//! un piège n'a donc aucun effet rétroactif.

use chrono::Utc;
use rusqlite::{params, Connection};

use crate::db::DbError;
use crate::sequences;
use crate::traps::EFFECTIVE_RECORDED_AT;

#[derive(Debug, serde::Serialize)]
pub struct VideoPosition {
    pub id: String,
    pub file_name: String,
    pub trap_id: String,
    pub trap_name: String,
    pub sequence_id: Option<String>,
    pub recorded_at: Option<String>,
    pub thumbnail_path: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude_m: Option<f64>,
    pub position_manual: bool,
    /// Position actuelle du piège, pour proposer « revenir au piège ».
    pub trap_latitude: Option<f64>,
    pub trap_longitude: Option<f64>,
}

/// Regroupe les vidéos par position distincte : ajuster 400 vidéos une par une n'a
/// aucun sens, alors qu'elles partagent presque toujours le même point.
#[derive(Debug, serde::Serialize)]
pub struct PositionGroup {
    pub trap_id: String,
    pub trap_name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub video_count: i64,
    pub video_ids: Vec<String>,
    pub first_at: Option<String>,
    pub last_at: Option<String>,
    pub any_manual: bool,
}

fn round(v: Option<f64>) -> Option<i64> {
    // Regroupement au cent-millième de degré (~1 m) : deux saisies manuelles du même
    // point ne doivent pas produire deux groupes.
    v.map(|v| (v * 100_000.0).round() as i64)
}

/// Les groupes de position des vidéos indexées lors du dernier import, ou de toutes
/// les vidéos d'un piège.
pub fn groups(
    conn: &Connection,
    trap_id: Option<&str>,
    only_since: Option<&str>,
) -> Result<Vec<PositionGroup>, DbError> {
    let mut clauses = vec!["v.deleted_at IS NULL".to_string()];
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(id) = trap_id {
        clauses.push("v.trap_id = ?".into());
        params.push(Box::new(id.to_string()));
    }
    if let Some(since) = only_since {
        clauses.push("v.imported_at >= ?".into());
        params.push(Box::new(since.to_string()));
    }
    let where_sql = clauses.join(" AND ");

    let sql = format!(
        "SELECT v.id, v.trap_id, t.name, v.latitude, v.longitude, v.position_manual, {eff}
         FROM videos v JOIN traps t ON t.id = v.trap_id
         WHERE {where_sql}
         ORDER BY t.name, {eff}",
        eff = EFFECTIVE_RECORDED_AT
    );
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(refs.as_slice())?;

    let mut out: Vec<PositionGroup> = Vec::new();
    while let Some(r) = rows.next()? {
        let id: String = r.get(0)?;
        let trap_id: String = r.get(1)?;
        let trap_name: String = r.get(2)?;
        let latitude: Option<f64> = r.get(3)?;
        let longitude: Option<f64> = r.get(4)?;
        let manual: bool = r.get::<_, i64>(5)? != 0;
        let at: Option<String> = r.get(6)?;

        let key = (trap_id.clone(), round(latitude), round(longitude));
        match out.iter_mut().find(|g| {
            (g.trap_id.clone(), round(g.latitude), round(g.longitude)) == key
        }) {
            Some(g) => {
                g.video_count += 1;
                g.video_ids.push(id);
                g.any_manual |= manual;
                if at.is_some() && (g.last_at.is_none() || at > g.last_at) {
                    g.last_at = at;
                }
            }
            None => out.push(PositionGroup {
                trap_id,
                trap_name,
                latitude,
                longitude,
                video_count: 1,
                video_ids: vec![id],
                first_at: at.clone(),
                last_at: at,
                any_manual: manual,
            }),
        }
    }

    out.sort_by(|a, b| b.video_count.cmp(&a.video_count));
    Ok(out)
}

/// Fixe la position d'un lot de vidéos. `None` efface la position.
pub fn set_positions(
    conn: &Connection,
    video_ids: &[String],
    latitude: Option<f64>,
    longitude: Option<f64>,
    altitude_m: Option<f64>,
) -> Result<usize, DbError> {
    if video_ids.is_empty() {
        return Err(DbError::Other("aucune vidéo sélectionnée".into()));
    }
    // Une latitude sans longitude pointerait au large de l'Afrique.
    if latitude.is_some() != longitude.is_some() {
        return Err(DbError::Other(
            "latitude et longitude vont ensemble : saisir les deux, ou aucune".into(),
        ));
    }
    if let (Some(lat), Some(lng)) = (latitude, longitude) {
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lng) {
            return Err(DbError::Other("coordonnées hors des bornes terrestres".into()));
        }
    }

    let now = Utc::now().to_rfc3339();
    let manual = latitude.is_some();
    let mut changed = 0;
    for id in video_ids {
        changed += conn.execute(
            "UPDATE videos SET latitude = ?2, longitude = ?3, altitude_m = ?4,
                 position_manual = ?5, updated_at = ?6
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, latitude, longitude, altitude_m, manual as i64, now],
        )?;
    }

    // Le lever et le coucher du soleil dépendent de la position : ils se recalculent.
    sequences::refresh_sun(conn)?;
    Ok(changed)
}

/// Combien de fichiers vidéo sont sous la racine sans être connus de la base.
///
/// Volontairement **approximatif et bon marché** : aucun calcul d'empreinte, aucune
/// lecture des métadonnées, seulement une comparaison de chemins. C'est une pastille
/// de notification, pas un inventaire — une vidéo simplement renommée y apparaît comme
/// nouvelle, et la passe d'indexation rétablira la vérité.
pub fn pending_count(conn: &Connection, root: &std::path::Path) -> Result<usize, DbError> {
    use std::collections::HashSet;

    if !root.is_dir() {
        return Ok(0);
    }

    let known: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT file_path FROM videos WHERE deleted_at IS NULL")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<_, _>>()?
    };

    let mut count = 0;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Même exemption de la racine que dans la passe d'indexation.
            e.depth() == 0
                || !e
                    .file_name()
                    .to_str()
                    .map(|n| n.starts_with('.') || n == "__MACOSX")
                    .unwrap_or(false)
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() || !crate::scan::is_video_path(entry.path()) {
            continue;
        }
        if !known.contains(&entry.path().display().to_string()) {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO traps (id, name, latitude, longitude, created_at, updated_at)
             VALUES ('t1', 'Mare basse', 47.32, 5.04, datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn
    }

    fn video(conn: &Connection, id: &str, lat: Option<f64>, lng: Option<f64>) {
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id,
                 recorded_at, latitude, longitude, imported_at, created_at, updated_at)
             VALUES (?1, ?1, ?1, ?1, 't1', '2026-03-14T21:00:00Z', ?2, ?3,
                     datetime('now'), datetime('now'), datetime('now'))",
            params![id, lat, lng],
        )
        .unwrap();
    }

    #[test]
    fn regroupe_les_videos_par_position() {
        let conn = db();
        video(&conn, "v1", Some(47.32), Some(5.04));
        video(&conn, "v2", Some(47.32), Some(5.04));
        video(&conn, "v3", Some(47.40), Some(5.10));

        let g = groups(&conn, None, None).unwrap();
        assert_eq!(g.len(), 2, "deux points distincts");
        assert_eq!(g[0].video_count, 2, "le groupe le plus fourni d'abord");
    }

    #[test]
    fn deux_saisies_du_meme_point_ne_font_qu_un_groupe() {
        let conn = db();
        // Un écart de moins d'un mètre.
        video(&conn, "v1", Some(47.320001), Some(5.040001));
        video(&conn, "v2", Some(47.320002), Some(5.040002));
        assert_eq!(groups(&conn, None, None).unwrap().len(), 1);
    }

    #[test]
    fn les_videos_sans_position_forment_leur_propre_groupe() {
        let conn = db();
        video(&conn, "v1", None, None);
        video(&conn, "v2", Some(47.32), Some(5.04));

        let g = groups(&conn, None, None).unwrap();
        assert_eq!(g.len(), 2);
        assert!(g.iter().any(|x| x.latitude.is_none()));
    }

    #[test]
    fn fixer_une_position_marque_la_saisie_manuelle() {
        let conn = db();
        video(&conn, "v1", Some(47.32), Some(5.04));
        set_positions(&conn, &["v1".into()], Some(47.5), Some(5.5), Some(310.0)).unwrap();

        let (lat, manual): (f64, i64) = conn
            .query_row(
                "SELECT latitude, position_manual FROM videos WHERE id = 'v1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(lat, 47.5);
        assert_eq!(manual, 1, "une passe d'indexation ne doit plus l'écraser");
    }

    #[test]
    fn refuse_une_coordonnee_a_moitie_saisie() {
        let conn = db();
        video(&conn, "v1", None, None);
        assert!(set_positions(&conn, &["v1".into()], Some(47.5), None, None).is_err());
    }

    #[test]
    fn refuse_des_coordonnees_hors_bornes() {
        let conn = db();
        video(&conn, "v1", None, None);
        assert!(set_positions(&conn, &["v1".into()], Some(120.0), Some(5.0), None).is_err());
    }

    #[test]
    fn compte_les_videos_pas_encore_indexees() {
        let conn = db();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Mare basse")).unwrap();
        std::fs::write(root.join("Mare basse/a.mp4"), b"x").unwrap();
        std::fs::write(root.join("Mare basse/b.mp4"), b"x").unwrap();
        std::fs::write(root.join("Mare basse/notes.txt"), b"x").unwrap();

        assert_eq!(pending_count(&conn, root).unwrap(), 2, "le .txt n'est pas une vidéo");

        // Une fois l'une connue, il n'en reste qu'une.
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id,
                 imported_at, created_at, updated_at)
             VALUES ('v1', ?1, 'a.mp4', 'h1', 't1',
                     datetime('now'), datetime('now'), datetime('now'))",
            params![root.join("Mare basse/a.mp4").display().to_string()],
        )
        .unwrap();
        assert_eq!(pending_count(&conn, root).unwrap(), 1);
    }

    #[test]
    fn une_racine_absente_ne_fait_pas_echouer_la_pastille() {
        let conn = db();
        assert_eq!(
            pending_count(&conn, std::path::Path::new("/nulle/part")).unwrap(),
            0,
            "un disque débranché ne doit pas casser l'écran d'accueil"
        );
    }
}

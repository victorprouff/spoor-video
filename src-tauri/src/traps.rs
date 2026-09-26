//! Les pièges : lieu durable (§3).

use chrono::Utc;
use rusqlite::{params, Connection};

use crate::db::DbError;

#[derive(Debug, serde::Serialize)]
pub struct Trap {
    pub id: String,
    pub name: String,
    pub folder_name: Option<String>,
    pub camera_name: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude_m: Option<f64>,
    pub clock_offset_minutes: i64,
    pub utc_offset_minutes: i64,
    pub notes: Option<String>,
    pub active: bool,
    pub video_count: i64,
    /// Bornes des captures rattachées, décalage d'horloge compris.
    pub first_video_at: Option<String>,
    pub last_video_at: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TrapInput {
    pub name: String,
    pub folder_name: Option<String>,
    pub camera_name: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude_m: Option<f64>,
    pub clock_offset_minutes: Option<i64>,
    pub utc_offset_minutes: Option<i64>,
    pub notes: Option<String>,
    pub active: Option<bool>,
}

/// Date de capture effective : la correction manuelle prime sur la donnée brute,
/// et le décalage d'horloge du piège s'applique à cette dernière seulement —
/// une date corrigée à la main l'a été en regardant l'heure vraie.
pub const EFFECTIVE_RECORDED_AT: &str = "COALESCE(v.recorded_at_manual,
     datetime(v.recorded_at, (t.clock_offset_minutes || ' minutes')))";

fn normalise(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn list(conn: &Connection) -> Result<Vec<Trap>, DbError> {
    let sql = format!(
        "SELECT t.id, t.name, t.folder_name, t.camera_name, t.latitude, t.longitude,
                t.altitude_m, t.clock_offset_minutes, t.utc_offset_minutes,
                t.notes, t.active,
                (SELECT COUNT(*) FROM videos v
                  WHERE v.trap_id = t.id AND v.deleted_at IS NULL),
                (SELECT MIN({eff}) FROM videos v
                  WHERE v.trap_id = t.id AND v.deleted_at IS NULL),
                (SELECT MAX({eff}) FROM videos v
                  WHERE v.trap_id = t.id AND v.deleted_at IS NULL)
         FROM traps t
         WHERE t.deleted_at IS NULL
         ORDER BY t.active DESC, t.name",
        eff = EFFECTIVE_RECORDED_AT
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        Ok(Trap {
            id: r.get(0)?,
            name: r.get(1)?,
            folder_name: r.get(2)?,
            camera_name: r.get(3)?,
            latitude: r.get(4)?,
            longitude: r.get(5)?,
            altitude_m: r.get(6)?,
            clock_offset_minutes: r.get(7)?,
            utc_offset_minutes: r.get(8)?,
            notes: r.get(9)?,
            active: r.get::<_, i64>(10)? != 0,
            video_count: r.get(11)?,
            first_video_at: r.get(12)?,
            last_video_at: r.get(13)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn check(input: &TrapInput) -> Result<(), DbError> {
    if input.name.trim().is_empty() {
        return Err(DbError::Other("un piège doit avoir un nom".into()));
    }
    // Des coordonnées à moitié saisies pointeraient au large de l'Afrique.
    if input.latitude.is_some() != input.longitude.is_some() {
        return Err(DbError::Other(
            "latitude et longitude vont ensemble : saisir les deux, ou aucune".into(),
        ));
    }
    if let (Some(lat), Some(lng)) = (input.latitude, input.longitude) {
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lng) {
            return Err(DbError::Other("coordonnées hors des bornes terrestres".into()));
        }
    }
    Ok(())
}

pub fn create(conn: &Connection, input: TrapInput) -> Result<String, DbError> {
    check(&input)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO traps (id, name, folder_name, camera_name, latitude, longitude,
             altitude_m, clock_offset_minutes, utc_offset_minutes, notes, active,
             created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
        params![
            id,
            input.name.trim(),
            normalise(input.folder_name),
            normalise(input.camera_name),
            input.latitude,
            input.longitude,
            input.altitude_m,
            input.clock_offset_minutes.unwrap_or(0),
            input.utc_offset_minutes.unwrap_or(60),
            normalise(input.notes),
            input.active.unwrap_or(true) as i64,
            now,
        ],
    )?;
    Ok(id)
}

pub fn update(conn: &Connection, id: &str, input: TrapInput) -> Result<(), DbError> {
    check(&input)?;
    let changed = conn.execute(
        "UPDATE traps SET name = ?2, folder_name = ?3, camera_name = ?4, latitude = ?5,
             longitude = ?6, altitude_m = ?7, clock_offset_minutes = ?8,
             utc_offset_minutes = ?9, notes = ?10, active = ?11, updated_at = ?12
         WHERE id = ?1 AND deleted_at IS NULL",
        params![
            id,
            input.name.trim(),
            normalise(input.folder_name),
            normalise(input.camera_name),
            input.latitude,
            input.longitude,
            input.altitude_m,
            input.clock_offset_minutes.unwrap_or(0),
            input.utc_offset_minutes.unwrap_or(60),
            normalise(input.notes),
            input.active.unwrap_or(true) as i64,
            Utc::now().to_rfc3339(),
        ],
    )?;
    if changed == 0 {
        return Err(DbError::Other("piège introuvable".into()));
    }
    Ok(())
}

/// Suppression douce. **Refusée tant que des vidéos y sont rattachées** : supprimer un
/// piège ne doit jamais rendre des captures orphelines. Un piège qu'on ne veut plus voir
/// se désactive (`active = 0`), il ne se supprime pas.
pub fn delete(conn: &Connection, id: &str) -> Result<(), DbError> {
    let videos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM videos WHERE trap_id = ?1 AND deleted_at IS NULL",
        [id],
        |r| r.get(0),
    )?;
    if videos > 0 {
        return Err(DbError::Other(format!(
            "{videos} vidéo(s) sont rattachées à ce piège, y compris celles supprimées en \
             gardant la trace. Pour le supprimer, supprime-les sans trace (onglet Vidéos, \
             filtré sur ce piège) ; pour garder leurs données, désactive-le plutôt"
        )));
    }
    conn.execute(
        "UPDATE traps SET deleted_at = ?2, folder_name = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        conn
    }

    fn input(name: &str) -> TrapInput {
        TrapInput {
            name: name.into(),
            folder_name: None,
            camera_name: None,
            latitude: None,
            longitude: None,
            altitude_m: None,
            clock_offset_minutes: None,
            utc_offset_minutes: None,
            notes: None,
            active: None,
        }
    }

    #[test]
    fn cree_et_liste() {
        let conn = db();
        create(&conn, input("Mare basse")).unwrap();
        let traps = list(&conn).unwrap();
        assert_eq!(traps.len(), 1);
        assert_eq!(traps[0].name, "Mare basse");
        assert!(traps[0].active);
        assert_eq!(traps[0].clock_offset_minutes, 0);
    }

    #[test]
    fn refuse_un_nom_vide() {
        let conn = db();
        assert!(create(&conn, input("   ")).is_err());
    }

    #[test]
    fn refuse_des_coordonnees_a_moitie_saisies() {
        let conn = db();
        let mut i = input("Mare basse");
        i.latitude = Some(47.3);
        assert!(
            create(&conn, i).is_err(),
            "une latitude sans longitude pointerait au large de l'Afrique"
        );
    }

    #[test]
    fn refuse_de_supprimer_un_piege_qui_porte_des_videos() {
        let conn = db();
        let id = create(&conn, input("Mare basse")).unwrap();
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id,
                 imported_at, created_at, updated_at)
             VALUES ('v1', '/a.mp4', 'a.mp4', 'h1', ?1, datetime('now'), datetime('now'), datetime('now'))",
            [&id],
        )
        .unwrap();
        assert!(delete(&conn, &id).is_err());
        // Sans vidéo, la suppression passe.
        conn.execute("DELETE FROM videos", []).unwrap();
        assert!(delete(&conn, &id).is_ok());
        assert!(list(&conn).unwrap().is_empty());
    }

    #[test]
    fn le_decalage_d_horloge_s_applique_a_la_lecture() {
        let conn = db();
        let mut i = input("Mare basse");
        i.clock_offset_minutes = Some(-60);
        let id = create(&conn, i).unwrap();
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id,
                 recorded_at, imported_at, created_at, updated_at)
             VALUES ('v1', '/a.mp4', 'a.mp4', 'h1', ?1, '2026-03-14T22:00:00Z',
                     datetime('now'), datetime('now'), datetime('now'))",
            [&id],
        )
        .unwrap();

        let traps = list(&conn).unwrap();
        assert_eq!(
            traps[0].first_video_at.as_deref(),
            Some("2026-03-14 21:00:00"),
            "l'heure affichée tient compte du décalage du piège"
        );

        // La donnée brute n'a pas bougé : le décalage se corrige sans réindexer.
        let brut: String = conn
            .query_row("SELECT recorded_at FROM videos", [], |r| r.get(0))
            .unwrap();
        assert_eq!(brut, "2026-03-14T22:00:00Z");
    }

    #[test]
    fn une_correction_manuelle_ignore_le_decalage() {
        let conn = db();
        let mut i = input("Mare basse");
        i.clock_offset_minutes = Some(-60);
        let id = create(&conn, i).unwrap();
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id,
                 recorded_at, recorded_at_manual, imported_at, created_at, updated_at)
             VALUES ('v1', '/a.mp4', 'a.mp4', 'h1', ?1, '2026-03-14T22:00:00Z',
                     '2026-03-14T18:30:00Z', datetime('now'), datetime('now'), datetime('now'))",
            [&id],
        )
        .unwrap();
        let traps = list(&conn).unwrap();
        assert_eq!(
            traps[0].first_video_at.as_deref(),
            Some("2026-03-14T18:30:00Z"),
            "une date saisie à la main l'a été en regardant l'heure vraie"
        );
    }
}

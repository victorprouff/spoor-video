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

/// Combien de vidéos d'un piège prendraient sa position actuelle.
///
/// `include_manual` dit s'il faut aussi réécrire celles dont la position a été choisie
/// à la main. Par défaut non : une correction manuelle est une décision, et une action
/// de masse ne doit pas l'effacer sans qu'on l'ait demandé.
pub fn trap_position_candidates(
    conn: &Connection,
    trap_id: &str,
    include_manual: bool,
) -> Result<i64, DbError> {
    let extra = if include_manual {
        ""
    } else {
        " AND position_manual = 0"
    };
    Ok(conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM videos
             WHERE trap_id = ?1 AND deleted_at IS NULL{extra}"
        ),
        [trap_id],
        |r| r.get(0),
    )?)
}

/// Applique la position actuelle du piège à ses vidéos déjà indexées.
///
/// Sert au cas courant : on renseigne les coordonnées d'un piège **après** avoir importé
/// ses vidéos. Sans cela, celles-ci resteraient sans position pour toujours, et ni le
/// rythme solaire ni le filtre jour/nuit ne fonctionneraient pour elles.
pub fn apply_trap_position(
    conn: &Connection,
    trap_id: &str,
    include_manual: bool,
) -> Result<usize, DbError> {
    let (lat, lng, alt): (Option<f64>, Option<f64>, Option<f64>) = conn.query_row(
        "SELECT latitude, longitude, altitude_m FROM traps WHERE id = ?1 AND deleted_at IS NULL",
        [trap_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if lat.is_none() || lng.is_none() {
        return Err(DbError::Other(
            "ce piège n'a pas de coordonnées : les renseigner d'abord".into(),
        ));
    }

    let extra = if include_manual {
        ""
    } else {
        " AND position_manual = 0"
    };
    let changed = conn.execute(
        &format!(
            "UPDATE videos SET latitude = ?2, longitude = ?3, altitude_m = ?4, updated_at = ?5
             WHERE trap_id = ?1 AND deleted_at IS NULL{extra}"
        ),
        params![trap_id, lat, lng, alt, Utc::now().to_rfc3339()],
    )?;

    sequences::refresh_sun(conn)?;
    Ok(changed)
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
    fn applique_la_position_du_piege_aux_videos_deja_indexees() {
        let conn = db();
        video(&conn, "v1", None, None);
        video(&conn, "v2", None, None);
        // Une vidéo dont la position a été choisie à la main.
        video(&conn, "v3", Some(47.9), Some(5.9));
        set_positions(&conn, &["v3".into()], Some(47.9), Some(5.9), None).unwrap();

        assert_eq!(trap_position_candidates(&conn, "t1", false).unwrap(), 2);
        assert_eq!(trap_position_candidates(&conn, "t1", true).unwrap(), 3);

        assert_eq!(apply_trap_position(&conn, "t1", false).unwrap(), 2);
        let manuelle: f64 = conn
            .query_row("SELECT latitude FROM videos WHERE id = 'v3'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            manuelle, 47.9,
            "une correction manuelle est une décision : une action de masse ne l'efface pas"
        );
        let reprise: f64 = conn
            .query_row("SELECT latitude FROM videos WHERE id = 'v1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(reprise, 47.32);
    }

    #[test]
    fn refuse_d_appliquer_la_position_d_un_piege_qui_n_en_a_pas() {
        let conn = db();
        conn.execute("UPDATE traps SET latitude = NULL, longitude = NULL", [])
            .unwrap();
        video(&conn, "v1", None, None);
        assert!(apply_trap_position(&conn, "t1", false).is_err());
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

}

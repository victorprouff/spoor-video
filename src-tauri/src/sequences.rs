//! Les séquences : le passage (§3).
//!
//! Un même animal déclenche souvent plusieurs vidéos d'affilée. Une séquence regroupe
//! les vidéos d'un même piège séparées de moins de `N` minutes.
//!
//! **Une séquence longue reste une séquence longue.** Si une espèce déclenche la caméra
//! pendant deux heures, c'est *un* passage de deux heures — avec sa durée et son nombre
//! de déclenchements conservés séparément — et non trente passages.

use chrono::Utc;
use rusqlite::{params, Connection};

use crate::db::DbError;
use crate::settings;
use crate::sun;
use crate::traps::EFFECTIVE_RECORDED_AT;

#[derive(Debug, Default, serde::Serialize)]
pub struct RegroupReport {
    /// Séquences reconstruites lors de cette passe.
    pub sequences_built: usize,
    /// Séquences laissées intactes : ajustées à la main, ou déjà dépouillées.
    pub sequences_frozen: usize,
    pub videos_grouped: usize,
    /// Vidéos sans date exploitable : sans place dans une chronologie, elles restent
    /// hors séquence plutôt que d'être rangées à un endroit inventé.
    pub videos_undated: usize,
    /// Séquences dont la position par rapport au soleil a pu être calculée.
    pub sun_computed: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct Sequence {
    pub id: String,
    pub trap_id: String,
    pub trap_name: String,
    pub started_at: String,
    pub ended_at: String,
    pub video_count: i64,
    /// Durée du passage, en secondes. C'est elle qui distingue un animal qui traverse
    /// d'un animal qui stationne deux heures.
    pub duration_s: i64,
    pub auto_grouped: bool,
    pub state: Option<String>,
    pub notes: Option<String>,
    pub reviewed_at: Option<String>,
}

/// Recalcule la position solaire de chaque séquence depuis les coordonnées de son piège.
/// Une séquence dont le piège n'a pas de position reste à NULL : on ne devine pas.
pub fn refresh_sun(conn: &Connection) -> Result<usize, DbError> {
    let rows: Vec<(String, String, f64, f64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.started_at, t.latitude, t.longitude, t.utc_offset_minutes
             FROM sequences s JOIN traps t ON t.id = s.trap_id
             WHERE s.deleted_at IS NULL
               AND t.latitude IS NOT NULL AND t.longitude IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let mut done = 0;
    for (id, started_at, lat, lng, utc_offset_minutes) in rows {
        let Some(at) = parse(&started_at) else { continue };
        let naive = at.naive_utc();
        let offset_hours = utc_offset_minutes as f64 / 60.0;
        let phase = sun::phase(naive, lat, lng, offset_hours);
        let delta = sun::minutes_from_sunset(naive, lat, lng, offset_hours);
        conn.execute(
            "UPDATE sequences SET sun_phase = ?2, minutes_from_sunset = ?3 WHERE id = ?1",
            params![id, phase.as_str(), delta.map(|d| d.round() as i64)],
        )?;
        done += 1;
    }
    Ok(done)
}

/// Une séquence est **gelée** — jamais reconstruite automatiquement — dès qu'elle porte
/// une décision humaine : un découpage manuel, une annotation, ou un dépouillement.
/// Sans cette règle, relancer une passe d'indexation effacerait du travail.
const FROZEN: &str = "(s.auto_grouped = 0
       OR s.state IS NOT NULL
       OR s.notes IS NOT NULL
       OR s.reviewed_at IS NOT NULL
       OR EXISTS (SELECT 1 FROM sequence_species ss WHERE ss.sequence_id = s.id)
       OR EXISTS (SELECT 1 FROM sequence_tags st WHERE st.sequence_id = s.id))";

/// Reconstruit les séquences des vidéos qui n'appartiennent à aucune séquence gelée.
pub fn regroup(conn: &Connection) -> Result<RegroupReport, DbError> {
    let gap_minutes = settings::sequence_gap_minutes(conn);
    let mut report = RegroupReport::default();

    // 1. Les séquences gelées restent telles quelles.
    let frozen: Vec<String> = {
        let sql = format!(
            "SELECT s.id FROM sequences s WHERE s.deleted_at IS NULL AND {FROZEN}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<_, _>>()?
    };
    report.sequences_frozen = frozen.len();

    // 2. Les autres sont défaites. Suppression franche et non douce : une séquence
    //    automatique sans annotation ne porte aucune donnée, seulement un regroupement
    //    recalculable. Le soft delete existe pour propager une suppression à des clients
    //    hors ligne — il n'y en a pas ici.
    let sql = format!(
        "UPDATE videos SET sequence_id = NULL
         WHERE sequence_id IN (SELECT s.id FROM sequences s
                               WHERE s.deleted_at IS NULL AND NOT {FROZEN})"
    );
    conn.execute(&sql, [])?;
    let sql = format!(
        "DELETE FROM sequences WHERE id IN (SELECT s.id FROM sequences s
                                            WHERE s.deleted_at IS NULL AND NOT {FROZEN})"
    );
    conn.execute(&sql, [])?;

    // 3. Les vidéos orphelines, dans l'ordre du temps, piège par piège.
    let sql = format!(
        "SELECT v.id, v.trap_id, {eff} AS at
         FROM videos v
         JOIN traps t ON t.id = v.trap_id
         WHERE v.deleted_at IS NULL AND v.sequence_id IS NULL AND {eff} IS NOT NULL
         ORDER BY v.trap_id, at",
        eff = EFFECTIVE_RECORDED_AT
    );
    let mut stmt = conn.prepare(&sql)?;
    let videos: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;

    let now = Utc::now().to_rfc3339();
    let mut current: Option<(String, String, Vec<String>)> = None; // (trap, dernière date, vidéos)

    for (video_id, trap_id, at) in videos {
        let continues = match &current {
            Some((t, last, _)) => t == &trap_id && within_gap(last, &at, gap_minutes),
            None => false,
        };

        if continues {
            let (_, last, ids) = current.as_mut().unwrap();
            *last = at;
            ids.push(video_id);
        } else {
            if let Some((trap, _, ids)) = current.take() {
                flush(conn, &trap, &ids, &now)?;
                report.sequences_built += 1;
                report.videos_grouped += ids.len();
            }
            current = Some((trap_id, at, vec![video_id]));
        }
    }
    if let Some((trap, _, ids)) = current.take() {
        flush(conn, &trap, &ids, &now)?;
        report.sequences_built += 1;
        report.videos_grouped += ids.len();
    }

    report.videos_undated = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM videos v JOIN traps t ON t.id = v.trap_id
             WHERE v.deleted_at IS NULL AND {eff} IS NULL",
            eff = EFFECTIVE_RECORDED_AT
        ),
        [],
        |r| r.get::<_, i64>(0),
    )? as usize;

    // Les bornes des séquences gelées peuvent avoir bougé (décalage d'horloge corrigé,
    // vidéo supprimée) : on les recalcule sans toucher à leur composition.
    for id in &frozen {
        refresh_bounds(conn, id)?;
    }

    // La position solaire dépend des coordonnées du piège, qui peuvent être saisies
    // après coup : on la recalcule pour TOUTES les séquences, gelées comprises.
    // C'est une donnée dérivée, pas une décision humaine — la réécrire ne détruit rien.
    report.sun_computed = refresh_sun(conn)?;

    Ok(report)
}

/// Les dates viennent de SQLite en `YYYY-MM-DD HH:MM:SS` ou en ISO-8601 selon qu'un
/// décalage d'horloge s'applique ou non : on normalise avant de comparer.
fn parse(raw: &str) -> Option<chrono::DateTime<Utc>> {
    crate::media::parse_timestamp(raw)
}

fn within_gap(previous: &str, next: &str, gap_minutes: i64) -> bool {
    match (parse(previous), parse(next)) {
        (Some(a), Some(b)) => (b - a).num_seconds().abs() <= gap_minutes * 60,
        // Une date illisible n'ouvre pas un regroupement au hasard : on coupe.
        _ => false,
    }
}

/// Crée une séquence et y rattache les vidéos.
fn flush(conn: &Connection, trap_id: &str, video_ids: &[String], now: &str) -> Result<(), DbError> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sequences (id, trap_id, started_at, ended_at, video_count,
             auto_grouped, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3, 0, 1, ?4, ?4)",
        params![id, trap_id, now, now],
    )?;
    for video_id in video_ids {
        conn.execute(
            "UPDATE videos SET sequence_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![video_id, id, now],
        )?;
    }
    refresh_bounds(conn, &id)?;
    Ok(())
}

/// Recalcule bornes et comptage d'une séquence depuis ses vidéos.
/// Une séquence vidée de ses vidéos est supprimée : elle ne représente plus rien.
pub fn refresh_bounds(conn: &Connection, sequence_id: &str) -> Result<(), DbError> {
    let sql = format!(
        "SELECT COUNT(*), MIN({eff}), MAX({eff})
         FROM videos v JOIN traps t ON t.id = v.trap_id
         WHERE v.sequence_id = ?1 AND v.deleted_at IS NULL",
        eff = EFFECTIVE_RECORDED_AT
    );
    let (count, min, max): (i64, Option<String>, Option<String>) =
        conn.query_row(&sql, [sequence_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;

    if count == 0 {
        conn.execute("DELETE FROM sequences WHERE id = ?1", [sequence_id])?;
        return Ok(());
    }

    conn.execute(
        "UPDATE sequences SET video_count = ?2, started_at = COALESCE(?3, started_at),
             ended_at = COALESCE(?4, ended_at), updated_at = ?5
         WHERE id = ?1",
        params![sequence_id, count, min, max, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

pub fn list(conn: &Connection, trap_id: Option<&str>) -> Result<Vec<Sequence>, DbError> {
    let mut sql = String::from(
        "SELECT s.id, s.trap_id, t.name, s.started_at, s.ended_at, s.video_count,
                s.auto_grouped, s.state, s.notes, s.reviewed_at
         FROM sequences s JOIN traps t ON t.id = s.trap_id
         WHERE s.deleted_at IS NULL",
    );
    if trap_id.is_some() {
        sql.push_str(" AND s.trap_id = ?1");
    }
    sql.push_str(" ORDER BY s.started_at DESC");

    let mut stmt = conn.prepare(&sql)?;
    let build = |r: &rusqlite::Row| -> rusqlite::Result<Sequence> {
        let started: String = r.get(3)?;
        let ended: String = r.get(4)?;
        let duration_s = match (parse(&started), parse(&ended)) {
            (Some(a), Some(b)) => (b - a).num_seconds().max(0),
            _ => 0,
        };
        Ok(Sequence {
            id: r.get(0)?,
            trap_id: r.get(1)?,
            trap_name: r.get(2)?,
            started_at: started,
            ended_at: ended,
            video_count: r.get(5)?,
            duration_s,
            auto_grouped: r.get::<_, i64>(6)? != 0,
            state: r.get(7)?,
            notes: r.get(8)?,
            reviewed_at: r.get(9)?,
        })
    };

    let rows: Vec<Sequence> = match trap_id {
        Some(id) => stmt.query_map([id], build)?.collect::<Result<_, _>>()?,
        None => stmt.query_map([], build)?.collect::<Result<_, _>>()?,
    };
    Ok(rows)
}

/// Scinde une séquence : `at_video_id` et tout ce qui suit passent dans une nouvelle
/// séquence. Les deux morceaux sont marqués manuels, donc gelés.
pub fn split(conn: &Connection, sequence_id: &str, at_video_id: &str) -> Result<String, DbError> {
    let sql = format!(
        "SELECT v.id FROM videos v JOIN traps t ON t.id = v.trap_id
         WHERE v.sequence_id = ?1 AND v.deleted_at IS NULL
         ORDER BY {eff}, v.file_name",
        eff = EFFECTIVE_RECORDED_AT
    );
    let ids: Vec<String> = {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([sequence_id], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let Some(cut) = ids.iter().position(|id| id == at_video_id) else {
        return Err(DbError::Other(
            "cette vidéo n'appartient pas à la séquence".into(),
        ));
    };
    if cut == 0 {
        return Err(DbError::Other(
            "scinder avant la première vidéo ne produirait qu'une séquence vide".into(),
        ));
    }

    let trap_id: String = conn.query_row(
        "SELECT trap_id FROM sequences WHERE id = ?1",
        [sequence_id],
        |r| r.get(0),
    )?;

    let now = Utc::now().to_rfc3339();
    let new_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sequences (id, trap_id, started_at, ended_at, video_count,
             auto_grouped, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3, 0, 0, ?4, ?4)",
        params![new_id, trap_id, now, now],
    )?;
    for id in &ids[cut..] {
        conn.execute(
            "UPDATE videos SET sequence_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, new_id, now],
        )?;
    }

    // Un découpage manuel doit survivre à toute passe ultérieure.
    conn.execute(
        "UPDATE sequences SET auto_grouped = 0, updated_at = ?2 WHERE id = ?1",
        params![sequence_id, now],
    )?;
    refresh_bounds(conn, sequence_id)?;
    refresh_bounds(conn, &new_id)?;
    Ok(new_id)
}

/// Fusionne plusieurs séquences en une. Le résultat est marqué manuel, donc gelé.
pub fn merge(conn: &Connection, sequence_ids: &[String]) -> Result<String, DbError> {
    if sequence_ids.len() < 2 {
        return Err(DbError::Other("il faut au moins deux séquences".into()));
    }

    let mut traps = std::collections::HashSet::new();
    for id in sequence_ids {
        let trap: String = conn
            .query_row("SELECT trap_id FROM sequences WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .map_err(|_| DbError::Other(format!("séquence introuvable : {id}")))?;
        traps.insert(trap);
    }
    if traps.len() > 1 {
        // Un passage a lieu devant un piège, pas devant deux : fusionner à travers des
        // emplacements produirait une séquence qui ne correspond à rien de réel.
        return Err(DbError::Other(
            "ces séquences appartiennent à des pièges différents".into(),
        ));
    }

    // La plus ancienne accueille les autres : garder le premier identifiant préserve
    // l'annotation déjà portée par le début du passage.
    let target = sequence_ids
        .iter()
        .min_by_key(|id| {
            conn.query_row(
                "SELECT started_at FROM sequences WHERE id = ?1",
                [id],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_default()
        })
        .unwrap()
        .clone();

    let now = Utc::now().to_rfc3339();
    for id in sequence_ids {
        if id == &target {
            continue;
        }
        conn.execute(
            "UPDATE videos SET sequence_id = ?2, updated_at = ?3 WHERE sequence_id = ?1",
            params![id, target, now],
        )?;
        // Les annotations des séquences absorbées suivent leurs vidéos.
        conn.execute(
            "INSERT OR IGNORE INTO sequence_species (sequence_id, species_id, confidence,
                 count_min, count_max, notes, created_at)
             SELECT ?2, species_id, confidence, count_min, count_max, notes, created_at
             FROM sequence_species WHERE sequence_id = ?1",
            params![id, target],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO sequence_tags (sequence_id, tag_id, created_at)
             SELECT ?2, tag_id, created_at FROM sequence_tags WHERE sequence_id = ?1",
            params![id, target],
        )?;
        conn.execute("DELETE FROM sequences WHERE id = ?1", [id])?;
    }

    conn.execute(
        "UPDATE sequences SET auto_grouped = 0, updated_at = ?2 WHERE id = ?1",
        params![target, now],
    )?;
    refresh_bounds(conn, &target)?;
    Ok(target)
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
            "INSERT INTO traps (id, name, created_at, updated_at)
             VALUES ('t1', 'Mare basse', datetime('now'), datetime('now')),
                    ('t2', 'Chablis nord', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn
    }

    /// Ajoute une vidéo à `minutes` minutes d'une origine fixe.
    fn video(conn: &Connection, id: &str, trap: &str, minutes: i64) {
        let at = chrono::DateTime::parse_from_rfc3339("2026-03-14T20:00:00Z").unwrap()
            + chrono::Duration::minutes(minutes);
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id,
                 recorded_at, imported_at, created_at, updated_at)
             VALUES (?1, ?1, ?1, ?1, ?2, ?3, datetime('now'), datetime('now'), datetime('now'))",
            params![id, trap, at.to_rfc3339()],
        )
        .unwrap();
    }

    fn videos_of(conn: &Connection, sequence_id: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT id FROM videos WHERE sequence_id = ?1 ORDER BY recorded_at")
            .unwrap();
        stmt.query_map([sequence_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn regroupe_par_ecart_de_temps() {
        let conn = db();
        // Trois déclenchements rapprochés, puis un passage bien plus tard.
        video(&conn, "a", "t1", 0);
        video(&conn, "b", "t1", 2);
        video(&conn, "c", "t1", 5);
        video(&conn, "d", "t1", 90);

        let r = regroup(&conn).unwrap();
        assert_eq!(r.sequences_built, 2);
        assert_eq!(r.videos_grouped, 4);

        let seqs = list(&conn, None).unwrap();
        assert_eq!(seqs.len(), 2);
        let long = seqs.iter().find(|s| s.video_count == 3).unwrap();
        assert_eq!(long.duration_s, 5 * 60, "la durée du passage est conservée");
    }

    #[test]
    fn une_activite_continue_reste_un_seul_passage() {
        let conn = db();
        // Une espèce déclenche toutes les 5 minutes pendant deux heures.
        for i in 0..25 {
            video(&conn, &format!("v{i}"), "t1", i * 5);
        }
        let r = regroup(&conn).unwrap();
        assert_eq!(r.sequences_built, 1, "un passage de deux heures, pas vingt-cinq");

        let seqs = list(&conn, None).unwrap();
        assert_eq!(seqs[0].video_count, 25, "le nombre de déclenchements est gardé");
        assert_eq!(seqs[0].duration_s, 120 * 60, "et la durée aussi");
    }

    #[test]
    fn ne_regroupe_pas_a_travers_deux_pieges() {
        let conn = db();
        video(&conn, "a", "t1", 0);
        video(&conn, "b", "t2", 1);
        let r = regroup(&conn).unwrap();
        assert_eq!(r.sequences_built, 2, "un passage a lieu devant un seul piège");
    }

    #[test]
    fn une_video_sans_date_reste_hors_sequence() {
        let conn = db();
        video(&conn, "a", "t1", 0);
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id,
                 imported_at, created_at, updated_at)
             VALUES ('sans', '/x', 'x', 'hx', 't1', datetime('now'), datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        let r = regroup(&conn).unwrap();
        assert_eq!(r.videos_undated, 1);
        let seq: Option<String> = conn
            .query_row("SELECT sequence_id FROM videos WHERE id = 'sans'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(
            seq.is_none(),
            "sans date, aucune place dans une chronologie : mieux vaut hors séquence \
             qu'à un endroit inventé"
        );
    }

    #[test]
    fn rejouer_le_regroupement_est_stable() {
        let conn = db();
        video(&conn, "a", "t1", 0);
        video(&conn, "b", "t1", 2);
        video(&conn, "c", "t1", 90);

        let premier = regroup(&conn).unwrap();
        let avant = list(&conn, None).unwrap().len();
        let second = regroup(&conn).unwrap();
        assert_eq!(premier.sequences_built, second.sequences_built);
        assert_eq!(list(&conn, None).unwrap().len(), avant);
    }

    #[test]
    fn une_sequence_annotee_n_est_jamais_defaite() {
        let conn = db();
        video(&conn, "a", "t1", 0);
        video(&conn, "b", "t1", 2);
        regroup(&conn).unwrap();

        let seq = list(&conn, None).unwrap()[0].id.clone();
        let species: String = conn
            .query_row("SELECT id FROM species LIMIT 1", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO sequence_species (sequence_id, species_id, confidence, created_at)
             VALUES (?1, ?2, 'certain', datetime('now'))",
            params![seq, species],
        )
        .unwrap();

        // Une nouvelle vidéo arrive, on relance : l'annotation doit survivre.
        video(&conn, "c", "t1", 200);
        let r = regroup(&conn).unwrap();
        assert_eq!(r.sequences_frozen, 1);

        let survivante: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sequences WHERE id = ?1",
                [&seq],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(survivante, 1, "relancer une passe n'efface pas du travail");
        assert_eq!(videos_of(&conn, &seq), vec!["a", "b"]);
    }

    #[test]
    fn scinder_puis_regrouper_conserve_le_decoupage() {
        let conn = db();
        video(&conn, "a", "t1", 0);
        video(&conn, "b", "t1", 2);
        video(&conn, "c", "t1", 4);
        regroup(&conn).unwrap();

        let seq = list(&conn, None).unwrap()[0].id.clone();
        let nouvelle = split(&conn, &seq, "b").unwrap();

        assert_eq!(videos_of(&conn, &seq), vec!["a"]);
        assert_eq!(videos_of(&conn, &nouvelle), vec!["b", "c"]);

        // Sans gel, la passe suivante recollerait tout : l'écart est de 2 minutes.
        let r = regroup(&conn).unwrap();
        assert_eq!(r.sequences_frozen, 2);
        assert_eq!(videos_of(&conn, &seq), vec!["a"]);
        assert_eq!(videos_of(&conn, &nouvelle), vec!["b", "c"]);
    }

    #[test]
    fn scinder_sur_la_premiere_video_est_refuse() {
        let conn = db();
        video(&conn, "a", "t1", 0);
        video(&conn, "b", "t1", 2);
        regroup(&conn).unwrap();
        let seq = list(&conn, None).unwrap()[0].id.clone();
        assert!(split(&conn, &seq, "a").is_err());
    }

    #[test]
    fn fusionner_recolle_et_gele() {
        let conn = db();
        video(&conn, "a", "t1", 0);
        video(&conn, "b", "t1", 60);
        regroup(&conn).unwrap();

        let seqs = list(&conn, None).unwrap();
        assert_eq!(seqs.len(), 2);
        let ids: Vec<String> = seqs.iter().map(|s| s.id.clone()).collect();
        let cible = merge(&conn, &ids).unwrap();

        let apres = list(&conn, None).unwrap();
        assert_eq!(apres.len(), 1);
        assert_eq!(apres[0].video_count, 2);
        assert!(!apres[0].auto_grouped);
        assert_eq!(apres[0].duration_s, 60 * 60);
        assert_eq!(videos_of(&conn, &cible), vec!["a", "b"]);

        // La passe suivante ne doit pas redécouper ce que j'ai recollé.
        regroup(&conn).unwrap();
        assert_eq!(list(&conn, None).unwrap().len(), 1);
    }

    #[test]
    fn fusionner_a_travers_deux_pieges_est_refuse() {
        let conn = db();
        video(&conn, "a", "t1", 0);
        video(&conn, "b", "t2", 0);
        regroup(&conn).unwrap();
        let ids: Vec<String> = list(&conn, None).unwrap().iter().map(|s| s.id.clone()).collect();
        assert!(
            merge(&conn, &ids).is_err(),
            "un passage a lieu devant un piège, pas devant deux"
        );
    }

    #[test]
    fn la_fusion_conserve_les_annotations_des_deux_cotes() {
        let conn = db();
        video(&conn, "a", "t1", 0);
        video(&conn, "b", "t1", 60);
        regroup(&conn).unwrap();

        let seqs = list(&conn, None).unwrap();
        // list() trie par started_at décroissant : la première est la plus récente.
        let (recente, ancienne) = (seqs[0].id.clone(), seqs[1].id.clone());
        let mut species = conn
            .prepare("SELECT id FROM species LIMIT 2")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .into_iter();
        let (s1, s2) = (species.next().unwrap(), species.next().unwrap());

        conn.execute(
            "INSERT INTO sequence_species (sequence_id, species_id, confidence, created_at)
             VALUES (?1, ?2, 'certain', datetime('now'))",
            params![ancienne, s1],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sequence_species (sequence_id, species_id, confidence, created_at)
             VALUES (?1, ?2, 'probable', datetime('now'))",
            params![recente, s2],
        )
        .unwrap();

        let cible = merge(&conn, &[ancienne.clone(), recente]).unwrap();
        assert_eq!(cible, ancienne, "la plus ancienne accueille les autres");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sequence_species WHERE sequence_id = ?1",
                [&cible],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2, "aucune identification n'est perdue à la fusion");
    }

    #[test]
    fn la_position_solaire_est_calculee_et_recalculable() {
        let conn = db();
        // Une séquence à 2 h du matin et une à 13 h, en juin, en Bourgogne.
        conn.execute(
            "INSERT INTO sequences (id, trap_id, started_at, ended_at, video_count,
                 auto_grouped, created_at, updated_at)
             VALUES ('nuit', 't1', '2026-06-21T02:00:00Z', '2026-06-21T02:00:00Z', 1, 1,
                     datetime('now'), datetime('now')),
                    ('jour', 't1', '2026-06-21T13:00:00Z', '2026-06-21T13:00:00Z', 1, 1,
                     datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        // Sans coordonnées, rien n'est calculé : on ne devine pas une position.
        assert_eq!(refresh_sun(&conn).unwrap(), 0);
        let phase: Option<String> = conn
            .query_row("SELECT sun_phase FROM sequences WHERE id = 'nuit'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(phase.is_none());

        // Les coordonnées saisies après coup suffisent : tout se recalcule.
        conn.execute(
            "UPDATE traps SET latitude = 47.32, longitude = 5.04, utc_offset_minutes = 120
             WHERE id = 't1'",
            [],
        )
        .unwrap();
        assert_eq!(refresh_sun(&conn).unwrap(), 2);

        let read = |id: &str| -> (String, i64) {
            conn.query_row(
                "SELECT sun_phase, minutes_from_sunset FROM sequences WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        assert_eq!(read("nuit").0, "night");
        assert_eq!(read("jour").0, "day");
        assert!(
            read("nuit").1 > 0,
            "2 h du matin est après le coucher, pas avant"
        );
    }

    #[test]
    fn le_decalage_d_horloge_est_pris_en_compte() {
        let conn = db();
        // Deux pièges, deux horloges : sans correction, t2 semble décalé d'une heure.
        video(&conn, "a", "t2", 0);
        video(&conn, "b", "t2", 61);
        conn.execute("UPDATE traps SET clock_offset_minutes = -60 WHERE id = 't2'", [])
            .unwrap();

        let r = regroup(&conn).unwrap();
        // Le décalage translate les deux dates pareillement : l'écart reste de 61 min,
        // donc deux séquences. C'est bien l'écart qui compte, pas l'heure absolue.
        assert_eq!(r.sequences_built, 2);

        let seqs = list(&conn, None).unwrap();
        assert!(
            seqs.iter().all(|s| s.started_at.starts_with("2026-03-14 19")
                || s.started_at.starts_with("2026-03-14 20")),
            "les bornes sont exprimées à l'heure corrigée, got {:?}",
            seqs.iter().map(|s| &s.started_at).collect::<Vec<_>>()
        );
    }
}

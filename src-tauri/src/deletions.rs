//! Supprimer une vidéo (§6).
//!
//! Trois cas, deux comportements. La distinction porte sur **ce qu'on garde**, jamais
//! sur la façon dont le fichier part — il part toujours à la corbeille du système.
//!
//! * **(a) sans trace** — la fausse déclenche. Le fichier et la ligne partent. L'empreinte
//!   est mémorisée pour que réimporter la même carte ne la fasse pas revenir.
//! * **(b) avec trace** — le passage humain. Le fichier part, la ligne reste : date,
//!   piège, séquence, espèce, tags. La séquence compte encore dans les statistiques.
//! * **(c) disparu hors de l'application** — détecté par la passe d'indexation (§4),
//!   fonctionnellement identique à (b), mais réversible si le fichier revient.

use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::{params, Connection};

use crate::db::DbError;
use crate::sequences;
use crate::settings;

/// Ce qui fait disparaître un fichier. Abstrait pour que les tests n'aient pas à
/// remplir la corbeille de la machine qui les exécute.
pub trait Disposer {
    fn dispose(&self, path: &Path) -> Result<(), String>;
}

/// La corbeille du système : récupérable tant qu'elle n'est pas vidée.
pub struct Trash;

impl Disposer for Trash {
    fn dispose(&self, path: &Path) -> Result<(), String> {
        trash::delete(path).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct DeleteReport {
    /// Fichiers envoyés à la corbeille.
    pub trashed: usize,
    /// Lignes supprimées de la base — uniquement dans le cas (a).
    pub rows_removed: usize,
    /// Vidéos dont le fichier était déjà parti : rien à envoyer à la corbeille,
    /// mais l'état est mis à jour.
    pub already_gone: usize,
    /// Séquences devenues vides, donc supprimées : elles ne décrivent plus rien.
    pub sequences_removed: usize,
    /// Identifications perdues avec les lignes supprimées (cas (a) uniquement).
    pub annotations_lost: usize,
    pub errors: Vec<String>,
}

/// Ce que l'on s'apprête à supprimer, pour que la confirmation dise vrai.
#[derive(Debug, Default, serde::Serialize)]
pub struct DeletePreview {
    pub videos: usize,
    pub present_files: usize,
    pub already_gone: usize,
    pub total_bytes: u64,
    pub sequences: usize,
    /// Séquences déjà dépouillées parmi celles visées.
    pub reviewed_sequences: usize,
    /// Identifications d'espèce portées par ces séquences.
    pub species_annotations: usize,
    /// Fichiers hors du dossier racine : ils ne seront pas touchés.
    pub outside_root: usize,
}

fn video_rows(
    conn: &Connection,
    video_ids: &[String],
) -> Result<Vec<(String, String, String, Option<i64>, String, Option<String>)>, DbError> {
    if video_ids.is_empty() {
        return Ok(Vec::new());
    }
    let holes = vec!["?"; video_ids.len()].join(",");
    let sql = format!(
        "SELECT id, file_path, file_name, file_size, file_state, sequence_id
         FROM videos WHERE id IN ({holes}) AND deleted_at IS NULL"
    );
    let params: Vec<&dyn rusqlite::ToSql> = video_ids
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
        ))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Les vidéos des séquences données.
pub fn videos_of_sequences(
    conn: &Connection,
    sequence_ids: &[String],
) -> Result<Vec<String>, DbError> {
    if sequence_ids.is_empty() {
        return Ok(Vec::new());
    }
    let holes = vec!["?"; sequence_ids.len()].join(",");
    let sql = format!(
        "SELECT id FROM videos WHERE sequence_id IN ({holes}) AND deleted_at IS NULL"
    );
    let params: Vec<&dyn rusqlite::ToSql> = sequence_ids
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Décrit ce que la suppression ferait. La confirmation doit annoncer des chiffres
/// exacts : « supprimer 34 vidéos » ne se vérifie pas après coup.
pub fn preview(conn: &Connection, video_ids: &[String]) -> Result<DeletePreview, DbError> {
    let root = settings::get(conn, settings::ROOT_PATH)?.map(PathBuf::from);
    let rows = video_rows(conn, video_ids)?;
    let mut p = DeletePreview {
        videos: rows.len(),
        ..Default::default()
    };

    let mut sequences = std::collections::HashSet::new();
    for (_, file_path, _, size, state, sequence_id) in &rows {
        if !on_disk(state, file_path) {
            p.already_gone += 1;
        } else if root.as_ref().is_some_and(|r| !Path::new(file_path).starts_with(r)) {
            p.outside_root += 1;
        } else {
            p.present_files += 1;
            p.total_bytes += size.unwrap_or(0) as u64;
        }
        if let Some(id) = sequence_id {
            sequences.insert(id.clone());
        }
    }
    p.sequences = sequences.len();

    if !sequences.is_empty() {
        let ids: Vec<String> = sequences.into_iter().collect();
        let holes = vec!["?"; ids.len()].join(",");
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        p.reviewed_sequences = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM sequences
                 WHERE id IN ({holes}) AND reviewed_at IS NOT NULL AND deleted_at IS NULL"
            ),
            params.as_slice(),
            |r| r.get(0),
        )?;
        p.species_annotations = conn.query_row(
            &format!("SELECT COUNT(*) FROM sequence_species WHERE sequence_id IN ({holes})"),
            params.as_slice(),
            |r| r.get(0),
        )?;
    }

    Ok(p)
}

/// **Cas (b)** : le fichier part à la corbeille, la ligne reste.
///
/// C'est l'usage courant — par défaut, dépouiller c'est garder la trace. La séquence
/// continue de compter dans les statistiques ; seule la lecture devient impossible.
pub fn delete_keeping_trace(
    conn: &Connection,
    video_ids: &[String],
    reason: Option<&str>,
    disposer: &dyn Disposer,
) -> Result<DeleteReport, DbError> {
    let mut report = DeleteReport::default();
    let rows = guarded_rows(conn, video_ids, &mut report)?;
    let now = Utc::now().to_rfc3339();
    let reason = reason.map(|r| r.trim()).filter(|r| !r.is_empty());

    for (id, file_path, file_name, _, state, _) in rows {
        if on_disk(&state, &file_path) {
            match disposer.dispose(Path::new(&file_path)) {
                Ok(()) => report.trashed += 1,
                Err(e) => {
                    // Un fichier qu'on n'a pas su déplacer ne doit pas être marqué
                    // supprimé : la base mentirait sur ce qu'il reste sur le disque.
                    report.errors.push(format!("{file_name} : {e}"));
                    continue;
                }
            }
        } else {
            report.already_gone += 1;
        }

        conn.execute(
            "UPDATE videos SET file_state = 'purged', file_removed_at = ?2,
                 file_removed_reason = ?3, updated_at = ?2
             WHERE id = ?1",
            params![id, now, reason],
        )?;
    }

    Ok(report)
}

/// **Cas (a)** : le fichier part à la corbeille **et** la ligne disparaît.
///
/// Réservé à ce qui n'a rien à dire. L'empreinte est mémorisée dans `purged_videos` :
/// sans cela, réimporter la même carte SD — qu'on n'efface pas forcément — ferait
/// revenir toutes les fausses déclenches déjà écartées, une par une, à chaque passe.
pub fn delete_without_trace(
    conn: &mut Connection,
    video_ids: &[String],
    disposer: &dyn Disposer,
) -> Result<DeleteReport, DbError> {
    let mut report = DeleteReport::default();
    let rows = guarded_rows(conn, video_ids, &mut report)?;

    // Les séquences concernées, pour recalculer leurs bornes après coup.
    let touched: Vec<String> = rows.iter().filter_map(|r| r.5.clone()).collect();

    // Ce qui part avec les lignes, compté avant de l'effacer.
    if !video_ids.is_empty() {
        let seqs: std::collections::HashSet<String> = touched.iter().cloned().collect();
        if !seqs.is_empty() {
            let ids: Vec<String> = seqs.into_iter().collect();
            let holes = vec!["?"; ids.len()].join(",");
            let params: Vec<&dyn rusqlite::ToSql> =
                ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
            report.annotations_lost = conn.query_row(
                &format!("SELECT COUNT(*) FROM sequence_species WHERE sequence_id IN ({holes})"),
                params.as_slice(),
                |r| r.get::<_, i64>(0),
            )? as usize;
        }
    }

    let now = Utc::now().to_rfc3339();
    let mut disposed: Vec<(String, String, String)> = Vec::new(); // (id, empreinte, chemin)

    for (id, file_path, file_name, _, state, _) in &rows {
        if on_disk(state, file_path) {
            match disposer.dispose(Path::new(file_path)) {
                Ok(()) => report.trashed += 1,
                Err(e) => {
                    report.errors.push(format!("{file_name} : {e}"));
                    continue;
                }
            }
        } else {
            report.already_gone += 1;
        }
        let hash: String = conn.query_row(
            "SELECT content_hash FROM videos WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?;
        disposed.push((id.clone(), hash, file_path.clone()));
    }

    // La mémoire du refus et la suppression de la ligne vont ensemble : si l'une passe
    // sans l'autre, la fausse déclenche revient à la passe suivante.
    let tx = conn.transaction()?;
    for (id, hash, file_path) in &disposed {
        let file_name: String =
            tx.query_row("SELECT file_name FROM videos WHERE id = ?1", [id], |r| {
                r.get(0)
            })?;
        tx.execute(
            "INSERT OR IGNORE INTO purged_videos (content_hash, file_name, purged_at, file_path)
             VALUES (?1, ?2, ?3, ?4)",
            params![hash, file_name, now, file_path],
        )?;
        tx.execute("DELETE FROM videos WHERE id = ?1", [id])?;
        report.rows_removed += 1;
    }
    tx.commit()?;

    // Une séquence vidée de ses vidéos ne décrit plus rien.
    let mut seen = std::collections::HashSet::new();
    for sequence_id in touched {
        if !seen.insert(sequence_id.clone()) {
            continue;
        }
        let before: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sequences WHERE id = ?1",
            [&sequence_id],
            |r| r.get(0),
        )?;
        sequences::refresh_bounds(conn, &sequence_id)?;
        let after: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sequences WHERE id = ?1",
            [&sequence_id],
            |r| r.get(0),
        )?;
        if before == 1 && after == 0 {
            report.sequences_removed += 1;
        }
    }

    Ok(report)
}

/// Y a-t-il un fichier à envoyer à la corbeille ?
///
/// Une vidéo encore `present` dont le fichier n'existe plus — hors de la racine, la
/// passe d'indexation ne l'a pas cherchée (§6 cas c) — n'a rien à toucher sur le disque.
/// Sans cela, sa ligne serait insupprimable, et son piège avec elle.
fn on_disk(state: &str, file_path: &str) -> bool {
    state == "present" && Path::new(file_path).exists()
}

/// Écarte ce qui ne doit pas être touché : un fichier hors du dossier racine.
///
/// Une ligne peut porter un chemin devenu faux (racine changée, base recopiée).
/// Supprimer sur la foi de ce chemin toucherait un fichier étranger à l'application.
fn guarded_rows(
    conn: &Connection,
    video_ids: &[String],
    report: &mut DeleteReport,
) -> Result<Vec<(String, String, String, Option<i64>, String, Option<String>)>, DbError> {
    if video_ids.is_empty() {
        return Err(DbError::Other("aucune vidéo sélectionnée".into()));
    }
    let root = settings::get(conn, settings::ROOT_PATH)?.map(PathBuf::from);
    let rows = video_rows(conn, video_ids)?;

    Ok(rows
        .into_iter()
        .filter(|(_, file_path, file_name, _, state, _)| {
            if !on_disk(state, file_path) {
                return true; // rien à toucher sur le disque
            }
            match &root {
                Some(root) if !Path::new(file_path).starts_with(root) => {
                    report.errors.push(format!(
                        "{file_name} : hors du dossier racine, non supprimé"
                    ));
                    false
                }
                _ => true,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{annotate, Annotation, SpeciesPick};
    use crate::db::migrations;
    use std::cell::RefCell;

    /// Supprime pour de bon, sans passer par la corbeille de la machine de test.
    struct FakeTrash {
        disposed: RefCell<Vec<PathBuf>>,
        fail: Option<String>,
    }

    impl FakeTrash {
        fn new() -> Self {
            Self {
                disposed: RefCell::new(Vec::new()),
                fail: None,
            }
        }
        fn failing(message: &str) -> Self {
            Self {
                disposed: RefCell::new(Vec::new()),
                fail: Some(message.into()),
            }
        }
    }

    impl Disposer for FakeTrash {
        fn dispose(&self, path: &Path) -> Result<(), String> {
            if let Some(e) = &self.fail {
                return Err(e.clone());
            }
            self.disposed.borrow_mut().push(path.to_path_buf());
            std::fs::remove_file(path).map_err(|e| e.to_string())
        }
    }

    struct Fixture {
        dir: tempfile::TempDir,
        conn: Connection,
    }

    fn db() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("videos");
        std::fs::create_dir_all(&root).unwrap();

        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        settings::set(&conn, settings::ROOT_PATH, &root.display().to_string()).unwrap();
        conn.execute(
            "INSERT INTO traps (id, name, created_at, updated_at)
             VALUES ('t1', 'Mare basse', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sequences (id, trap_id, started_at, ended_at, video_count,
                 auto_grouped, created_at, updated_at)
             VALUES ('s1', 't1', '2026-03-14 21:00:00', '2026-03-14 21:05:00', 0, 1,
                     datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        Fixture { dir, conn }
    }

    /// Crée un fichier et sa ligne. `inside` place le fichier sous la racine ou non.
    fn video(f: &Fixture, id: &str, inside: bool) -> PathBuf {
        let dir = if inside {
            f.dir.path().join("videos/Mare basse")
        } else {
            f.dir.path().join("ailleurs")
        };
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{id}.mp4"));
        std::fs::write(&path, b"des octets de video").unwrap();

        f.conn
            .execute(
                "INSERT INTO videos (id, file_path, file_name, file_size, content_hash,
                     trap_id, sequence_id, recorded_at, file_state, imported_at,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, 19, ?4, 't1', 's1', '2026-03-14T21:00:00Z', 'present',
                         datetime('now'), datetime('now'), datetime('now'))",
                params![id, path.display().to_string(), format!("{id}.mp4"), format!("h-{id}")],
            )
            .unwrap();
        sequences::refresh_bounds(&f.conn, "s1").unwrap();
        path
    }

    fn state_of(conn: &Connection, id: &str) -> Option<String> {
        conn.query_row("SELECT file_state FROM videos WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .ok()
    }

    #[test]
    fn avec_trace_le_fichier_part_la_ligne_reste() {
        let f = db();
        let path = video(&f, "v1", true);
        let bin = FakeTrash::new();

        let r = delete_keeping_trace(&f.conn, &["v1".into()], Some("passage humain"), &bin).unwrap();
        assert_eq!(r.trashed, 1);
        assert_eq!(r.rows_removed, 0);
        assert!(!path.exists(), "le fichier est parti");

        let (state, reason): (String, Option<String>) = f
            .conn
            .query_row(
                "SELECT file_state, file_removed_reason FROM videos WHERE id = 'v1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, "purged");
        assert_eq!(reason.as_deref(), Some("passage humain"));

        // La séquence survit : elle compte encore dans les statistiques.
        let seqs: i64 = f
            .conn
            .query_row("SELECT COUNT(*) FROM sequences", [], |r| r.get(0))
            .unwrap();
        assert_eq!(seqs, 1);
    }

    #[test]
    fn sans_trace_la_ligne_part_et_l_empreinte_reste() {
        let mut f = db();
        let path = video(&f, "v1", true);
        let bin = FakeTrash::new();

        let r = delete_without_trace(&mut f.conn, &["v1".into()], &bin).unwrap();
        assert_eq!(r.trashed, 1);
        assert_eq!(r.rows_removed, 1);
        assert!(!path.exists());

        let videos: i64 = f
            .conn
            .query_row("SELECT COUNT(*) FROM videos", [], |r| r.get(0))
            .unwrap();
        assert_eq!(videos, 0);

        let purged: i64 = f
            .conn
            .query_row("SELECT COUNT(*) FROM purged_videos WHERE content_hash = 'h-v1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            purged, 1,
            "sans mémoire du refus, la fausse déclenche revient au prochain import"
        );
    }

    #[test]
    fn une_sequence_videe_disparait() {
        let mut f = db();
        video(&f, "v1", true);
        video(&f, "v2", true);
        let bin = FakeTrash::new();

        let r = delete_without_trace(&mut f.conn, &["v1".into()], &bin).unwrap();
        assert_eq!(r.sequences_removed, 0, "il reste une vidéo");

        let r = delete_without_trace(&mut f.conn, &["v2".into()], &bin).unwrap();
        assert_eq!(r.sequences_removed, 1, "plus rien à décrire");
        let seqs: i64 = f
            .conn
            .query_row("SELECT COUNT(*) FROM sequences", [], |r| r.get(0))
            .unwrap();
        assert_eq!(seqs, 0);
    }

    #[test]
    fn un_fichier_hors_de_la_racine_n_est_jamais_touche() {
        let f = db();
        let path = video(&f, "v1", false);
        let bin = FakeTrash::new();

        let r = delete_keeping_trace(&f.conn, &["v1".into()], None, &bin).unwrap();
        assert_eq!(r.trashed, 0);
        assert!(path.exists(), "un fichier étranger à la racine reste intact");
        assert_eq!(r.errors.len(), 1);
        assert_eq!(
            state_of(&f.conn, "v1").as_deref(),
            Some("present"),
            "et la base ne prétend pas l'avoir supprimé"
        );
    }

    #[test]
    fn une_video_hors_racine_dont_le_fichier_n_existe_plus_se_supprime() {
        // Le cas des vidéos d'essai : indexées depuis un dossier disparu depuis, restées
        // `present` faute d'avoir été cherchées. Elles bloquaient aussi leur piège.
        let mut f = db();
        let path = video(&f, "v1", false);
        std::fs::remove_file(&path).unwrap();
        let bin = FakeTrash::new();

        let p = preview(&f.conn, &["v1".into()]).unwrap();
        assert_eq!((p.present_files, p.already_gone, p.outside_root), (0, 1, 0));

        let r = delete_without_trace(&mut f.conn, &["v1".into()], &bin).unwrap();
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!((r.trashed, r.already_gone, r.rows_removed), (0, 1, 1));
        assert!(bin.disposed.borrow().is_empty());
        assert_eq!(state_of(&f.conn, "v1"), None);
    }

    #[test]
    fn une_video_present_dont_le_fichier_manque_garde_sa_trace() {
        let f = db();
        let path = video(&f, "v1", true);
        std::fs::remove_file(&path).unwrap();
        let bin = FakeTrash::new();

        let r = delete_keeping_trace(&f.conn, &["v1".into()], None, &bin).unwrap();
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.already_gone, 1);
        assert_eq!(state_of(&f.conn, "v1").as_deref(), Some("purged"));
    }

    #[test]
    fn un_echec_de_corbeille_ne_marque_pas_la_video_supprimee() {
        let f = db();
        let path = video(&f, "v1", true);
        let bin = FakeTrash::failing("corbeille pleine");

        let r = delete_keeping_trace(&f.conn, &["v1".into()], None, &bin).unwrap();
        assert_eq!(r.trashed, 0);
        assert_eq!(r.errors.len(), 1);
        assert!(path.exists());
        assert_eq!(
            state_of(&f.conn, "v1").as_deref(),
            Some("present"),
            "la base mentirait sur ce qu'il reste sur le disque"
        );
    }

    #[test]
    fn une_video_deja_disparue_est_comptee_pas_reessayee() {
        let f = db();
        video(&f, "v1", true);
        f.conn
            .execute("UPDATE videos SET file_state = 'missing' WHERE id = 'v1'", [])
            .unwrap();
        let bin = FakeTrash::new();

        let r = delete_keeping_trace(&f.conn, &["v1".into()], None, &bin).unwrap();
        assert_eq!(r.trashed, 0);
        assert_eq!(r.already_gone, 1);
        assert_eq!(state_of(&f.conn, "v1").as_deref(), Some("purged"));
        assert!(bin.disposed.borrow().is_empty());
    }

    #[test]
    fn l_apercu_annonce_des_chiffres_exacts() {
        let mut f = db();
        video(&f, "v1", true);
        video(&f, "v2", true);
        video(&f, "v3", false);

        let species: String = f
            .conn
            .query_row("SELECT id FROM species LIMIT 1", [], |r| r.get(0))
            .unwrap();
        annotate(
            &mut f.conn,
            &["s1".into()],
            Annotation {
                add_species: vec![SpeciesPick {
                    species_id: species,
                    confidence: "certain".into(),
                    count_min: None,
                    count_max: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();

        let p = preview(&f.conn, &["v1".into(), "v2".into(), "v3".into()]).unwrap();
        assert_eq!(p.videos, 3);
        assert_eq!(p.present_files, 2, "le fichier hors racine ne partira pas");
        assert_eq!(p.total_bytes, 38);
        assert_eq!(p.sequences, 1);
        assert_eq!(p.reviewed_sequences, 1, "la séquence est dépouillée");
        assert_eq!(p.species_annotations, 1);
        assert_eq!(p.outside_root, 1, "un fichier ne sera pas touché, il faut le dire");
    }

    #[test]
    fn une_selection_vide_est_refusee() {
        let f = db();
        let bin = FakeTrash::new();
        assert!(delete_keeping_trace(&f.conn, &[], None, &bin).is_err());
    }

    #[test]
    fn les_videos_d_une_sequence_se_retrouvent() {
        let f = db();
        video(&f, "v1", true);
        video(&f, "v2", true);
        let mut ids = videos_of_sequences(&f.conn, &["s1".into()]).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["v1", "v2"]);
    }
}

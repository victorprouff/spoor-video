//! La passe d'indexation (§4).
//!
//! Principe : **rien n'est une liste entretenue.** La file de travail est le résultat
//! d'une comparaison entre le disque et la base, recalculée à chaque passe. Une passe
//! est donc rejouable sans dommage, et déposer des fichiers dans un dossier de caméra
//! suffit à les mettre dans la file.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use walkdir::WalkDir;

use crate::db::DbError;
use crate::{hash, media, sequences};

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "avi", "mov", "mkv", "m4v", "mpg", "mpeg"];

/// Compte rendu d'une passe. Chaque nombre correspond à une décision prise, jamais
/// à un silence : ce qui n'a pas pu être traité est compté, pas oublié.
#[derive(Debug, Default, serde::Serialize)]
pub struct ScanReport {
    pub root_path: String,
    pub files_seen: usize,
    pub files_added: usize,
    pub files_known: usize,
    /// Fichiers retrouvés à un autre chemin : la ligne est mise à jour, pas dupliquée.
    pub files_moved: usize,
    /// Fichiers déjà supprimés sans trace et réapparus (§6 cas a).
    pub files_repurged: usize,
    /// Vidéos de la base dont le fichier a disparu : passées en `missing` (§6 cas c).
    pub files_missing: usize,
    /// Vidéos `missing` dont le fichier est revenu : repassées en `present`.
    pub files_recovered: usize,
    /// Fichiers sans date exploitable : indexés, marqués, **jamais datés d'office**.
    pub files_no_date: usize,
    pub files_error: usize,
    /// Dossiers de premier niveau non rattachés à un piège : rien n'a été importé
    /// de ces dossiers, ils attendent une décision.
    pub unknown_folders: Vec<String>,
    pub errors: Vec<String>,
    pub ffmpeg_available: bool,
    /// Séquences reconstruites à l'issue de la passe (étape 7 du §4).
    pub sequences_built: usize,
    /// Séquences laissées intactes parce qu'ajustées ou annotées à la main.
    pub sequences_frozen: usize,
}

/// Correspondance dossier de premier niveau → piège, telle qu'elle est en base.
fn trap_by_folder(conn: &Connection) -> Result<HashMap<String, String>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT folder_name, id FROM traps WHERE folder_name IS NOT NULL AND deleted_at IS NULL",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut map = HashMap::new();
    for row in rows {
        let (folder, id) = row?;
        map.insert(folder, id);
    }
    Ok(map)
}

fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Le dossier de premier niveau sous la racine — celui qui porte le piège.
/// Tout ce qu'il contient, y compris en sous-dossiers (les cartes SD créent souvent
/// leurs propres niveaux), hérite du même piège.
fn top_folder(root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    let mut components = relative.components();
    let first = components.next()?;
    // Il faut au moins un composant après : sans quoi `first` est le fichier lui-même,
    // posé directement à la racine, et non un dossier de caméra.
    components.next()?;
    Some(first.as_os_str().to_string_lossy().to_string()).filter(|n| !n.is_empty())
}

pub fn scan(conn: &Connection, root: &Path, thumbs_dir: &Path) -> Result<ScanReport, DbError> {
    let mut report = ScanReport {
        root_path: root.display().to_string(),
        ffmpeg_available: media::available(),
        ..Default::default()
    };

    if !root.is_dir() {
        return Err(DbError::Other(format!(
            "racine introuvable : {}",
            root.display()
        )));
    }

    let import_id = uuid::Uuid::new_v4().to_string();
    let started_at = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO imports (id, root_path, started_at, created_at)
         VALUES (?1, ?2, ?3, ?3)",
        params![import_id, root.display().to_string(), started_at],
    )?;

    let traps = trap_by_folder(conn)?;
    let mut unknown: HashSet<String> = HashSet::new();
    // Chemins rencontrés sur le disque, pour le contrôle inverse (étape 8 du §4).
    let mut seen_hashes: HashSet<String> = HashSet::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Les dossiers cachés et les rebuts macOS ne sont pas des vidéos.
            !e.file_name()
                .to_str()
                .map(|n| n.starts_with('.') || n == "__MACOSX")
                .unwrap_or(false)
        })
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !entry.file_type().is_file() || !is_video(path) {
            continue;
        }
        report.files_seen += 1;

        let Some(folder) = top_folder(root, path) else {
            // Vidéo posée à la racine, hors de tout dossier de caméra : on ne devine pas
            // à quel piège elle appartient.
            unknown.insert("(racine)".to_string());
            continue;
        };

        let Some(trap_id) = traps.get(&folder) else {
            // Dossier inconnu : rien n'est importé, aucun piège n'est créé en silence.
            unknown.insert(folder);
            continue;
        };

        match index_one(conn, path, trap_id, thumbs_dir, &mut report) {
            Ok(Some(h)) => {
                seen_hashes.insert(h);
            }
            Ok(None) => {}
            Err(e) => {
                report.files_error += 1;
                report
                    .errors
                    .push(format!("{} : {e}", path.file_name().unwrap().to_string_lossy()));
            }
        }
    }

    report.unknown_folders = {
        let mut v: Vec<String> = unknown.into_iter().collect();
        v.sort();
        v
    };

    // Contrôle inverse : les vidéos que la base connaît mais que le disque n'a plus.
    // Une vidéo déjà `purged` (§6 cas b) n'est pas concernée : son fichier est parti
    // volontairement, ce n'est pas une disparition.
    report.files_missing = mark_missing(conn, root, &mut report)?;

    // Étape 7 : regroupement en séquences. Les séquences ajustées ou annotées à la main
    // sont gelées, donc relancer une passe ne peut pas effacer du travail.
    let regroup = sequences::regroup(conn)?;
    report.sequences_built = regroup.sequences_built;
    report.sequences_frozen = regroup.sequences_frozen;

    conn.execute(
        "UPDATE imports SET finished_at = ?2, files_seen = ?3, files_added = ?4,
             files_known = ?5, files_repurged = ?6, files_missing = ?7,
             files_no_date = ?8, files_error = ?9, unknown_folders = ?10
         WHERE id = ?1",
        params![
            import_id,
            Utc::now().to_rfc3339(),
            report.files_seen as i64,
            report.files_added as i64,
            report.files_known as i64,
            report.files_repurged as i64,
            report.files_missing as i64,
            report.files_no_date as i64,
            report.files_error as i64,
            report.unknown_folders.join("\n"),
        ],
    )?;

    Ok(report)
}

/// Traite un fichier. Rend son empreinte quand il correspond à une ligne vivante.
fn index_one(
    conn: &Connection,
    path: &Path,
    trap_id: &str,
    thumbs_dir: &Path,
    report: &mut ScanReport,
) -> Result<Option<String>, DbError> {
    let content_hash = hash::content_hash(path)?;

    // Supprimé sans trace une fois : ne doit pas revenir à chaque réimport de la carte.
    let purged: Option<String> = conn
        .query_row(
            "SELECT content_hash FROM purged_videos WHERE content_hash = ?1",
            [&content_hash],
            |r| r.get(0),
        )
        .optional()?;
    if purged.is_some() {
        report.files_repurged += 1;
        return Ok(None);
    }

    let existing: Option<(String, String, String)> = conn
        .query_row(
            "SELECT id, file_path, file_state FROM videos
             WHERE content_hash = ?1 AND deleted_at IS NULL",
            [&content_hash],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;

    let now = Utc::now().to_rfc3339();
    let file_path = path.display().to_string();
    let file_name = path.file_name().unwrap().to_string_lossy().to_string();

    if let Some((id, known_path, state)) = existing {
        // Connu. Le chemin peut avoir changé (dossier renommé, vidéo rangée ailleurs) :
        // c'est le même fichier, on met le chemin à jour plutôt que de créer un doublon.
        if known_path != file_path {
            report.files_moved += 1;
        } else {
            report.files_known += 1;
        }
        // Un fichier revenu après une disparition redevient `present` tout seul :
        // débrancher un disque externe ne détruit rien (§6 cas c).
        if state == "missing" {
            report.files_recovered += 1;
        }
        conn.execute(
            "UPDATE videos SET file_path = ?2, file_name = ?3,
                 file_state = CASE WHEN file_state = 'missing' THEN 'present' ELSE file_state END,
                 updated_at = ?4
             WHERE id = ?1",
            params![id, file_path, file_name, now],
        )?;
        return Ok(Some(content_hash));
    }

    // Nouveau fichier.
    let probe = if media::available() {
        match media::probe(path) {
            Ok(p) => p,
            Err(e) => {
                // Un fichier illisible est compté et signalé, jamais inséré avec des
                // caractéristiques inventées.
                return Err(DbError::Other(e.to_string()));
            }
        }
    } else {
        media::Probe::default()
    };

    if probe.recorded_at.is_none() {
        report.files_no_date += 1;
    }

    let id = uuid::Uuid::new_v4().to_string();
    let file_size = std::fs::metadata(path).map(|m| m.len() as i64).ok();

    // La vignette est un agrément, pas une donnée : son échec ne doit pas faire perdre
    // l'indexation du fichier.
    let thumbnail_path = make_thumbnail(path, &id, thumbs_dir, probe.duration_s)
        .map_err(|e| log::warn!("vignette impossible pour {file_name} : {e}"))
        .ok();

    conn.execute(
        "INSERT INTO videos (id, file_path, file_name, file_size, content_hash, trap_id,
             recorded_at, duration_s, width, height, fps, thumbnail_path,
             file_state, imported_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'present', ?13, ?13, ?13)",
        params![
            id,
            file_path,
            file_name,
            file_size,
            content_hash,
            trap_id,
            probe.recorded_at.map(|d| d.to_rfc3339()),
            probe.duration_s,
            probe.width,
            probe.height,
            probe.fps,
            thumbnail_path,
            now,
        ],
    )?;
    report.files_added += 1;

    Ok(Some(content_hash))
}

fn make_thumbnail(
    src: &Path,
    video_id: &str,
    thumbs_dir: &Path,
    duration_s: Option<f64>,
) -> Result<String, media::MediaError> {
    // Un peu après le début : sur un piège photo la première image est souvent noire,
    // l'infrarouge n'ayant pas encore éclairé la scène.
    let at = duration_s.map(|d| (d * 0.15).clamp(0.0, 3.0)).unwrap_or(1.0);
    let dest = thumbs_dir.join(format!("{video_id}.jpg"));
    media::thumbnail(src, &dest, at, 480)?;
    Ok(dest.display().to_string())
}

/// Passe en `missing` les vidéos dont le fichier n'est plus là (§6 cas c).
fn mark_missing(conn: &Connection, root: &Path, report: &mut ScanReport) -> Result<usize, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path FROM videos
         WHERE deleted_at IS NULL AND file_state = 'present'",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;

    let now = Utc::now().to_rfc3339();
    let mut count = 0;
    for (id, file_path) in rows {
        let path = PathBuf::from(&file_path);
        if path.exists() {
            continue;
        }
        // Un fichier hors de la racine balayée n'a pas été cherché : le déclarer disparu
        // serait faux. C'est le cas d'un second disque simplement non monté.
        if !path.starts_with(root) {
            continue;
        }
        conn.execute(
            "UPDATE videos SET file_state = 'missing', file_removed_at = ?2, updated_at = ?2
             WHERE id = ?1",
            params![id, now],
        )?;
        count += 1;
    }
    let _ = report;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnait_les_extensions_video() {
        assert!(is_video(Path::new("/a/IMG_0001.MP4")));
        assert!(is_video(Path::new("/a/b.avi")));
        assert!(!is_video(Path::new("/a/notes.txt")));
        assert!(!is_video(Path::new("/a/sans-extension")));
    }

    #[test]
    fn le_dossier_de_premier_niveau_porte_le_piege() {
        let root = Path::new("/videos");
        assert_eq!(
            top_folder(root, Path::new("/videos/Mare basse/a.mp4")).as_deref(),
            Some("Mare basse")
        );
        // Les sous-dossiers de la carte SD héritent du même piège.
        assert_eq!(
            top_folder(root, Path::new("/videos/Mare basse/DCIM/100MEDIA/a.mp4")).as_deref(),
            Some("Mare basse")
        );
        // Une vidéo posée à la racine n'appartient à aucun piège.
        assert_eq!(top_folder(root, Path::new("/videos/a.mp4")).as_deref(), None);
    }
}

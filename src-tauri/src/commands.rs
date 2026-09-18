//! Les commandes appelées par le front.

use std::path::PathBuf;

use chrono::Utc;
use rusqlite::params;
use tauri::Manager;

use crate::db::{Db, DbError};
use crate::scan::{self, ScanReport};
use crate::{media, settings};

#[derive(serde::Serialize)]
pub struct Trap {
    id: String,
    name: String,
    folder_name: Option<String>,
    video_count: i64,
}

#[derive(serde::Serialize)]
pub struct AppState {
    root_path: Option<String>,
    ffmpeg_available: bool,
    traps: Vec<Trap>,
    videos_count: i64,
    videos_missing: i64,
    videos_no_date: i64,
    last_scan: Option<String>,
}

#[tauri::command]
pub fn app_state(db: tauri::State<'_, Db>) -> Result<AppState, DbError> {
    let conn = db.conn.lock().unwrap();

    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.folder_name,
                (SELECT COUNT(*) FROM videos v WHERE v.trap_id = t.id AND v.deleted_at IS NULL)
         FROM traps t WHERE t.deleted_at IS NULL ORDER BY t.name",
    )?;
    let traps = stmt
        .query_map([], |r| {
            Ok(Trap {
                id: r.get(0)?,
                name: r.get(1)?,
                folder_name: r.get(2)?,
                video_count: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(AppState {
        root_path: settings::get(&conn, settings::ROOT_PATH)?,
        ffmpeg_available: media::available(),
        traps,
        videos_count: conn.query_row(
            "SELECT COUNT(*) FROM videos WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )?,
        videos_missing: conn.query_row(
            "SELECT COUNT(*) FROM videos WHERE deleted_at IS NULL AND file_state = 'missing'",
            [],
            |r| r.get(0),
        )?,
        videos_no_date: conn.query_row(
            "SELECT COUNT(*) FROM videos
             WHERE deleted_at IS NULL AND recorded_at IS NULL AND recorded_at_manual IS NULL",
            [],
            |r| r.get(0),
        )?,
        last_scan: conn
            .query_row(
                "SELECT finished_at FROM imports WHERE finished_at IS NOT NULL
                 ORDER BY finished_at DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .ok(),
    })
}

#[tauri::command]
pub fn set_root_path(db: tauri::State<'_, Db>, path: String) -> Result<(), DbError> {
    let candidate = PathBuf::from(&path);
    if !candidate.is_dir() {
        return Err(DbError::Other(format!("dossier introuvable : {path}")));
    }
    let conn = db.conn.lock().unwrap();
    settings::set(&conn, settings::ROOT_PATH, &path)?;
    Ok(())
}

/// Rattache un dossier de premier niveau à un piège : soit un piège existant,
/// soit un nouveau piège portant le nom du dossier. Jamais fait automatiquement (§4).
#[tauri::command]
pub fn link_folder_to_trap(
    db: tauri::State<'_, Db>,
    folder_name: String,
    trap_id: Option<String>,
) -> Result<String, DbError> {
    let conn = db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();

    let id = match trap_id {
        Some(id) => {
            conn.execute(
                "UPDATE traps SET folder_name = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, folder_name, now],
            )?;
            id
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO traps (id, name, folder_name, created_at, updated_at)
                 VALUES (?1, ?2, ?2, ?3, ?3)",
                params![id, folder_name, now],
            )?;
            id
        }
    };
    Ok(id)
}

/// Lance une passe d'indexation. Bloquante : le front affiche une attente.
/// Le suivi de progression viendra quand des lots réels auront montré ce qui est lent.
#[tauri::command]
pub async fn scan_root(app: tauri::AppHandle) -> Result<ScanReport, DbError> {
    let thumbs_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| DbError::Other(e.to_string()))?
        .join("thumbnails");

    // Le balayage est du disque et des sous-processus : le sortir du fil de l'interface
    // évite de figer la fenêtre pendant plusieurs minutes.
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        let conn = db.conn.lock().unwrap();
        let root = settings::get(&conn, settings::ROOT_PATH)?
            .ok_or_else(|| DbError::Other("aucune racine configurée".into()))?;
        scan::scan(&conn, &PathBuf::from(root), &thumbs_dir)
    })
    .await
    .map_err(|e| DbError::Other(format!("passe interrompue : {e}")))?
}

//! Les commandes appelées par le front.

use std::path::PathBuf;

use chrono::Utc;
use rusqlite::params;
use tauri::Manager;

use crate::annotations::{self, AnnotateReport, Annotation};
use crate::db::{Db, DbError};
use crate::deletions::{self, DeletePreview, DeleteReport, Trash};
use crate::export::{self, CopyReport, ExportReport};
use crate::grid::{self, GridFilter, GridPage};
use crate::pending::{self, DiscardedFile, PendingReport};
use crate::positions;
use crate::stats::{self, Stats};
use crate::videos::{self, VideoFilter, VideoPage};
use crate::scan::{self, ScanReport};
use crate::sequences::{self, RegroupReport, Sequence};
use crate::species::{self, Species, SpeciesInput};
use crate::traps::{self, Trap, TrapInput};
use crate::{media, settings};

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

    Ok(AppState {
        root_path: settings::get(&conn, settings::ROOT_PATH)?,
        ffmpeg_available: media::available(),
        traps: traps::list(&conn)?,
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
pub fn set_root_path(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    path: String,
) -> Result<(), DbError> {
    let candidate = PathBuf::from(&path);
    if !candidate.is_dir() {
        return Err(DbError::Other(format!("dossier introuvable : {path}")));
    }
    {
        let conn = db.conn.lock().unwrap();
        settings::set(&conn, settings::ROOT_PATH, &path)?;
    }
    // La nouvelle racine devient lisible immédiatement, sans redémarrer.
    app.asset_protocol_scope()
        .allow_directory(&path, true)
        .map_err(|e| DbError::Other(format!("racine non autorisée à la lecture : {e}")))?;
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

// --- Pièges ---------------------------------------------------------------

#[tauri::command]
pub fn list_traps(db: tauri::State<'_, Db>) -> Result<Vec<Trap>, DbError> {
    traps::list(&db.conn.lock().unwrap())
}

#[tauri::command]
pub fn create_trap(db: tauri::State<'_, Db>, input: TrapInput) -> Result<String, DbError> {
    traps::create(&db.conn.lock().unwrap(), input)
}

#[tauri::command]
pub fn update_trap(
    db: tauri::State<'_, Db>,
    id: String,
    input: TrapInput,
) -> Result<(), DbError> {
    traps::update(&db.conn.lock().unwrap(), &id, input)
}

#[tauri::command]
pub fn delete_trap(db: tauri::State<'_, Db>, id: String) -> Result<(), DbError> {
    traps::delete(&db.conn.lock().unwrap(), &id)
}

/// Les dossiers de premier niveau présents sous la racine, avec ce à quoi ils sont
/// rattachés — pour rattacher sans avoir à relancer une passe.
#[tauri::command]
pub fn list_root_folders(db: tauri::State<'_, Db>) -> Result<Vec<RootFolder>, DbError> {
    let conn = db.conn.lock().unwrap();
    let Some(root) = settings::get(&conn, settings::ROOT_PATH)? else {
        return Ok(Vec::new());
    };

    let mut linked = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT folder_name, name FROM traps
             WHERE folder_name IS NOT NULL AND deleted_at IS NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (folder, trap) = row?;
            linked.insert(folder, trap);
        }
    }

    let mut folders = Vec::new();
    let entries = std::fs::read_dir(&root)
        .map_err(|e| DbError::Other(format!("racine illisible : {e}")))?;
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        folders.push(RootFolder {
            trap_name: linked.get(&name).cloned(),
            name,
        });
    }
    folders.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(folders)
}

#[derive(serde::Serialize)]
pub struct RootFolder {
    pub name: String,
    pub trap_name: Option<String>,
}

// --- Espèces --------------------------------------------------------------

#[tauri::command]
pub fn list_species(db: tauri::State<'_, Db>) -> Result<Vec<Species>, DbError> {
    species::list(&db.conn.lock().unwrap())
}

#[tauri::command]
pub fn create_species(db: tauri::State<'_, Db>, input: SpeciesInput) -> Result<String, DbError> {
    species::create(&db.conn.lock().unwrap(), input)
}

#[tauri::command]
pub fn update_species(
    db: tauri::State<'_, Db>,
    id: String,
    input: SpeciesInput,
) -> Result<(), DbError> {
    species::update(&db.conn.lock().unwrap(), &id, input)
}

#[tauri::command]
pub fn delete_species(db: tauri::State<'_, Db>, id: String) -> Result<(), DbError> {
    species::delete(&db.conn.lock().unwrap(), &id)
}

// --- Séquences ------------------------------------------------------------

#[tauri::command]
pub fn list_sequences(
    db: tauri::State<'_, Db>,
    trap_id: Option<String>,
) -> Result<Vec<Sequence>, DbError> {
    sequences::list(&db.conn.lock().unwrap(), trap_id.as_deref())
}

#[tauri::command]
pub fn regroup_sequences(db: tauri::State<'_, Db>) -> Result<RegroupReport, DbError> {
    sequences::regroup(&db.conn.lock().unwrap())
}

#[tauri::command]
pub fn split_sequence(
    db: tauri::State<'_, Db>,
    sequence_id: String,
    at_video_id: String,
) -> Result<String, DbError> {
    sequences::split(&db.conn.lock().unwrap(), &sequence_id, &at_video_id)
}

#[tauri::command]
pub fn merge_sequences(
    db: tauri::State<'_, Db>,
    sequence_ids: Vec<String>,
) -> Result<String, DbError> {
    sequences::merge(&db.conn.lock().unwrap(), &sequence_ids)
}

/// Les vidéos d'une séquence, dans l'ordre du temps.
#[tauri::command]
pub fn list_sequence_videos(
    db: tauri::State<'_, Db>,
    sequence_id: String,
) -> Result<Vec<SequenceVideo>, DbError> {
    let conn = db.conn.lock().unwrap();
    let sql = format!(
        "SELECT v.id, v.file_name, v.file_path, v.thumbnail_path, {eff}, v.duration_s,
                v.file_state
         FROM videos v JOIN traps t ON t.id = v.trap_id
         WHERE v.sequence_id = ?1 AND v.deleted_at IS NULL
         ORDER BY {eff}, v.file_name",
        eff = crate::traps::EFFECTIVE_RECORDED_AT
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([&sequence_id], |r| {
        Ok(SequenceVideo {
            id: r.get(0)?,
            file_name: r.get(1)?,
            file_path: r.get(2)?,
            thumbnail_path: r.get(3)?,
            recorded_at: r.get(4)?,
            duration_s: r.get(5)?,
            file_state: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[derive(serde::Serialize)]
pub struct SequenceVideo {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub thumbnail_path: Option<String>,
    pub recorded_at: Option<String>,
    pub duration_s: Option<f64>,
    pub file_state: String,
}

// --- Grille et annotation -------------------------------------------------

#[tauri::command]
pub fn grid_page(db: tauri::State<'_, Db>, filter: GridFilter) -> Result<GridPage, DbError> {
    grid::page(&db.conn.lock().unwrap(), filter)
}

#[tauri::command]
pub fn annotate_sequences(
    db: tauri::State<'_, Db>,
    sequence_ids: Vec<String>,
    annotation: Annotation,
) -> Result<AnnotateReport, DbError> {
    annotations::annotate(&mut db.conn.lock().unwrap(), &sequence_ids, annotation)
}


// --- Statistiques ---------------------------------------------------------

#[tauri::command]
pub fn stats(db: tauri::State<'_, Db>, filter: GridFilter) -> Result<Stats, DbError> {
    stats::compute(&db.conn.lock().unwrap(), filter)
}

// --- Exports --------------------------------------------------------------

#[tauri::command]
pub fn export_sequences_csv(
    db: tauri::State<'_, Db>,
    filter: GridFilter,
    path: String,
) -> Result<ExportReport, DbError> {
    export::sequences_csv(&db.conn.lock().unwrap(), filter, &PathBuf::from(path))
}

#[tauri::command]
pub fn export_detections_csv(
    db: tauri::State<'_, Db>,
    filter: GridFilter,
    path: String,
) -> Result<ExportReport, DbError> {
    export::detections_csv(&db.conn.lock().unwrap(), filter, &PathBuf::from(path))
}

/// Copie les vidéos des séquences choisies. Bloquant : des vidéos pèsent lourd.
#[tauri::command]
pub async fn copy_videos(
    app: tauri::AppHandle,
    sequence_ids: Vec<String>,
    dest_dir: String,
) -> Result<CopyReport, DbError> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        let conn = db.conn.lock().unwrap();
        export::copy_videos(&conn, &sequence_ids, &PathBuf::from(dest_dir))
    })
    .await
    .map_err(|e| DbError::Other(format!("copie interrompue : {e}")))?
}

// --- Suppressions (§6) ----------------------------------------------------

#[tauri::command]
pub fn videos_of_sequences(
    db: tauri::State<'_, Db>,
    sequence_ids: Vec<String>,
) -> Result<Vec<String>, DbError> {
    deletions::videos_of_sequences(&db.conn.lock().unwrap(), &sequence_ids)
}

#[tauri::command]
pub fn preview_deletion(
    db: tauri::State<'_, Db>,
    video_ids: Vec<String>,
) -> Result<DeletePreview, DbError> {
    deletions::preview(&db.conn.lock().unwrap(), &video_ids)
}

#[tauri::command]
pub fn delete_videos_keeping_trace(
    db: tauri::State<'_, Db>,
    video_ids: Vec<String>,
    reason: Option<String>,
) -> Result<DeleteReport, DbError> {
    deletions::delete_keeping_trace(
        &db.conn.lock().unwrap(),
        &video_ids,
        reason.as_deref(),
        &Trash,
    )
}

#[tauri::command]
pub fn delete_videos_without_trace(
    db: tauri::State<'_, Db>,
    video_ids: Vec<String>,
) -> Result<DeleteReport, DbError> {
    deletions::delete_without_trace(&mut db.conn.lock().unwrap(), &video_ids, &Trash)
}

// --- Positions (migration 005) --------------------------------------------

#[tauri::command]
pub fn set_video_positions(
    db: tauri::State<'_, Db>,
    video_ids: Vec<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    altitude_m: Option<f64>,
) -> Result<usize, DbError> {
    positions::set_positions(
        &db.conn.lock().unwrap(),
        &video_ids,
        latitude,
        longitude,
        altitude_m,
    )
}

/// Pastille « des vidéos attendent ». Elle dit ce que la passe ferait vraiment.
#[tauri::command]
pub fn pending_videos(db: tauri::State<'_, Db>) -> Result<PendingReport, DbError> {
    let conn = db.conn.lock().unwrap();
    let Some(root) = settings::get(&conn, settings::ROOT_PATH)? else {
        return Ok(PendingReport::default());
    };
    pending::report(&conn, &PathBuf::from(root))
}

/// Les vidéos écartées sans trace dont le fichier est revenu sur le disque.
#[tauri::command]
pub fn discarded_files(db: tauri::State<'_, Db>) -> Result<Vec<DiscardedFile>, DbError> {
    pending::discarded_files(&db.conn.lock().unwrap())
}

#[tauri::command]
pub fn restore_discarded(db: tauri::State<'_, Db>, content_hash: String) -> Result<(), DbError> {
    pending::restore(&db.conn.lock().unwrap(), &content_hash)
}

// --- Vue vidéo ------------------------------------------------------------

#[tauri::command]
pub fn list_videos(db: tauri::State<'_, Db>, filter: VideoFilter) -> Result<VideoPage, DbError> {
    videos::page(&db.conn.lock().unwrap(), filter)
}

/// Combien de vidéos prendraient la position actuelle du piège.
#[tauri::command]
pub fn trap_position_candidates(
    db: tauri::State<'_, Db>,
    trap_id: String,
    include_manual: bool,
) -> Result<i64, DbError> {
    positions::trap_position_candidates(&db.conn.lock().unwrap(), &trap_id, include_manual)
}

#[tauri::command]
pub fn apply_trap_position(
    db: tauri::State<'_, Db>,
    trap_id: String,
    include_manual: bool,
) -> Result<usize, DbError> {
    positions::apply_trap_position(&db.conn.lock().unwrap(), &trap_id, include_manual)
}

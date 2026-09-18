mod annotations;
mod commands;
mod db;
mod deletions;
mod export;
mod grid;
mod hash;
mod media;
mod positions;
mod scan;
mod sequences;
mod settings;
mod sun;
mod species;
mod stats;
mod traps;
mod videos;

use tauri::Manager;

use db::{Db, DbError};

/// Ce que le front affiche au démarrage pour vérifier que le socle tient debout.
#[derive(serde::Serialize)]
pub struct DbStatus {
    db_path: String,
    schema_version: i64,
    species_count: i64,
    traps_count: i64,
    videos_count: i64,
    sequences_count: i64,
}

#[tauri::command]
fn db_status(db: tauri::State<'_, Db>) -> Result<DbStatus, DbError> {
    let conn = db.conn.lock().unwrap();
    let count = |table: &str| -> Result<i64, rusqlite::Error> {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
    };

    Ok(DbStatus {
        db_path: db.path.display().to_string(),
        schema_version: conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?,
        species_count: count("species")?,
        traps_count: count("traps")?,
        videos_count: count("videos")?,
        sequences_count: count("sequences")?,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Sans logger, tous les log::warn! de l'application partent au néant — et une
        // application installée n'a pas de sortie standard où les lire. On écrit donc
        // dans un fichier, à côté de la base.
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("spoor-video".into()),
                    },
                ))
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Stderr,
                ))
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // La base vit dans le dossier de données de l'application, jamais à côté
            // des vidéos : le drive peut être débranché, la base doit rester lisible.
            let dir = app.path().app_data_dir()?;
            let db = Db::open(&dir)?;
            log::info!("base ouverte : {}", db.path.display());
            app.manage(db);

            // ffmpeg est cherché une fois, au démarrage : d'abord embarqué dans
            // l'application, puis aux emplacements usuels (une app lancée depuis le
            // Finder n'hérite pas du PATH du shell).
            media::init(app.path().resource_dir().ok().as_deref());

            // Lire une vidéo dans la fenêtre suppose d'ouvrir son dossier au protocole
            // `asset`. On n'ouvre que **la racine configurée**, et seulement si elle
            // existe : la webview n'a aucune raison de lire ailleurs sur le disque.
            {
                let db = app.state::<Db>();
                let conn = db.conn.lock().unwrap();
                if let Ok(Some(root)) = settings::get(&conn, settings::ROOT_PATH) {
                    if let Err(e) = app.asset_protocol_scope().allow_directory(&root, true) {
                        log::warn!("racine non autorisée à la lecture : {e}");
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_status,
            commands::app_state,
            commands::set_root_path,
            commands::link_folder_to_trap,
            commands::scan_root,
            commands::list_traps,
            commands::create_trap,
            commands::update_trap,
            commands::delete_trap,
            commands::list_root_folders,
            commands::list_species,
            commands::create_species,
            commands::update_species,
            commands::delete_species,
            commands::list_sequences,
            commands::regroup_sequences,
            commands::split_sequence,
            commands::merge_sequences,
            commands::list_sequence_videos,
            commands::grid_page,
            commands::annotate_sequences,
            commands::stats,
            commands::export_sequences_csv,
            commands::export_detections_csv,
            commands::copy_videos,
            commands::preview_deletion,
            commands::delete_videos_keeping_trace,
            commands::delete_videos_without_trace,
            commands::videos_of_sequences,
            commands::position_groups,
            commands::set_video_positions,
            commands::pending_videos,
            commands::list_videos,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}

/// Surface exposée aux tests d'intégration. Ces fonctions ne sont pas une API :
/// elles existent pour que la passe d'indexation soit testable sur de vrais fichiers,
/// sans lancer l'application.
#[doc(hidden)]
pub mod testing {
    pub use crate::db::migrations::apply as apply_migrations;
    pub use crate::media::{available as media_available, init as media_init};
    pub use crate::scan::{scan, ScanReport};
    pub use crate::annotations::{annotate, Annotation, SpeciesPick};
    pub use crate::export::{copy_videos, detections_csv, sequences_csv};
    pub use crate::grid::GridFilter;
    pub use crate::sequences::{list as list_sequences, Sequence};
}

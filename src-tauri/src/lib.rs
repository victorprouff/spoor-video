mod annotations;
mod commands;
mod db;
mod grid;
mod hash;
mod media;
mod scan;
mod sequences;
mod settings;
mod species;
mod traps;

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
            commands::list_tags,
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
    pub use crate::sequences::{list as list_sequences, Sequence};
}

mod db;
mod settings;

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
    /// Racine des vidéos, si elle a déjà été choisie (§4).
    root_path: Option<String>,
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
        root_path: settings::get(&conn, settings::ROOT_PATH)?,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // La base vit dans le dossier de données de l'application, jamais à côté
            // des vidéos : le drive peut être débranché, la base doit rester lisible.
            let dir = app.path().app_data_dir()?;
            let db = Db::open(&dir)?;
            log::info!("base ouverte : {}", db.path.display());
            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![db_status])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}

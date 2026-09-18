pub mod migrations;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

/// Connexion unique à la base, partagée par les commandes Tauri.
/// Mono-poste, mono-utilisateur : un `Mutex` suffit, inutile de sortir un pool.
pub struct Db {
    pub conn: Mutex<Connection>,
    pub path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("erreur SQLite : {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("erreur d'entrée-sortie : {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

impl serde::Serialize for DbError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl Db {
    /// Ouvre `spoor-video.sqlite` dans `dir`, applique les réglages puis les migrations.
    pub fn open(dir: &Path) -> Result<Self, DbError> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("spoor-video.sqlite");
        let conn = Connection::open(&path)?;

        // WAL : la lecture ne bloque pas l'écriture, ce qui compte dès qu'une passe
        // d'indexation tourne pendant qu'on dépouille.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // `full` plutôt que `normal` : une coupure de courant ne doit pas coûter
        // une passe d'indexation entière.
        conn.pragma_update(None, "synchronous", "full")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        migrations::apply(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            path,
        })
    }
}

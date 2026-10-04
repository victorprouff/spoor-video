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

        // `full` plutôt que `normal` : une coupure de courant ne doit pas coûter
        // une passe d'indexation entière.
        conn.pragma_update(None, "synchronous", "full")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        // Avant tout changement de mode : une base plus récente que l'application est
        // refusée sans qu'on y ait écrit quoi que ce soit.
        migrations::apply(&conn)?;

        // Journal classique, pas WAL. Le WAL laisse à côté de la base deux fichiers
        // (`-wal`, `-shm`) qui en font partie : un dossier synchronisé (kDrive) peut
        // envoyer l'un sans l'autre, et l'autre machine reçoit une base corrompue. En
        // `delete`, une fois l'écriture finie, la base tient dans son seul fichier. Le WAL
        // n'apportait rien ici : une seule connexion, derrière un `Mutex`. Passer une base
        // WAL en `delete` y reverse le contenu de `-wal`, puis le supprime.
        conn.pragma_update(None, "journal_mode", "DELETE")?;

        Ok(Self {
            conn: Mutex::new(conn),
            path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_base_tient_dans_un_seul_fichier() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let conn = db.conn.lock().unwrap();
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, "delete");
        conn.execute(
            "INSERT INTO traps (id, name, created_at, updated_at) VALUES ('t', 'Mare', 'x', 'x')",
            [],
        )
        .unwrap();
        assert!(!dir.path().join("spoor-video.sqlite-wal").exists());
        assert!(!dir.path().join("spoor-video.sqlite-journal").exists());
    }

    #[test]
    fn une_ancienne_base_wal_passe_en_journal_classique_sans_rien_perdre() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spoor-video.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "journal_mode", "WAL").unwrap();
            migrations::apply(&conn).unwrap();
            conn.execute(
                "INSERT INTO traps (id, name, created_at, updated_at) VALUES ('t', 'Mare', 'x', 'x')",
                [],
            )
            .unwrap();
        }
        let db = Db::open(dir.path()).unwrap();
        let conn = db.conn.lock().unwrap();
        let name: String = conn.query_row("SELECT name FROM traps", [], |r| r.get(0)).unwrap();
        assert_eq!(name, "Mare");
        assert!(!dir.path().join("spoor-video.sqlite-wal").exists());
    }
}

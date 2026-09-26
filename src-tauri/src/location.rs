//! Où vivent la base et les vignettes.
//!
//! Par défaut, dans le dossier de données de l'application. On peut les déplacer
//! n'importe où, par exemple à côté des vidéos. Le choix est retenu dans un petit
//! fichier `location.json`, qui reste, lui, dans le dossier de données : la base ne
//! peut pas dire elle-même où elle se trouve.
//!
//! En développement, tout vit dans un sous-dossier `dev/`, avec son propre
//! `location.json` : une séance de travail ne touche jamais les vraies données.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::db::{Db, DbError};

pub const DB_FILE: &str = "spoor-video.sqlite";
const THUMBS_DIR: &str = "thumbnails";
const CONFIG_FILE: &str = "location.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct Config {
    data_dir: PathBuf,
}

/// L'emplacement retenu au démarrage, et ce qui s'y est passé.
#[derive(Clone, serde::Serialize)]
pub struct DataLocation {
    /// Dossier de données de l'application : l'emplacement par défaut, et celui de
    /// `location.json`.
    pub home: PathBuf,
    /// Dossier qui contient la base et les vignettes.
    pub dir: PathBuf,
    pub is_default: bool,
    pub dev: bool,
    /// Pourquoi la base n'a pas pu être ouverte. `None` si elle l'est.
    pub error: Option<String>,
}

impl DataLocation {
    pub fn db_path(&self) -> PathBuf {
        self.dir.join(DB_FILE)
    }

    pub fn thumbs_dir(&self) -> PathBuf {
        self.dir.join(THUMBS_DIR)
    }
}

/// Le dossier de données : celui de l'application, ou son sous-dossier `dev/`.
pub fn home(app_data_dir: &Path) -> PathBuf {
    if cfg!(debug_assertions) {
        app_data_dir.join("dev")
    } else {
        app_data_dir.to_path_buf()
    }
}

/// Lit `location.json`. Un fichier illisible n'est pas ignoré : repartir sur
/// l'emplacement par défaut ouvrirait une base vide à la place de la vraie.
pub fn resolve(home: &Path) -> DataLocation {
    let mut loc = DataLocation {
        home: home.to_path_buf(),
        dir: home.to_path_buf(),
        is_default: true,
        dev: cfg!(debug_assertions),
        error: None,
    };
    let config = home.join(CONFIG_FILE);
    if !config.exists() {
        return loc;
    }
    match std::fs::read_to_string(&config)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str::<Config>(&s).map_err(|e| e.to_string()))
    {
        Ok(c) => {
            loc.is_default = c.data_dir == home;
            loc.dir = c.data_dir;
        }
        Err(e) => loc.error = Some(format!("{} est illisible : {e}", config.display())),
    }
    loc
}

/// Ouvre la base à l'emplacement retenu.
///
/// Ailleurs qu'à l'emplacement par défaut, rien n'est créé : si le dossier ou la base
/// manquent, c'est que le disque est débranché ou le dossier déplacé, et ouvrir une
/// base neuve ferait croire que tout le travail a disparu.
pub fn open(loc: &DataLocation) -> Result<Db, String> {
    if let Some(e) = &loc.error {
        return Err(e.clone());
    }
    if !loc.is_default {
        if !loc.dir.is_dir() {
            return Err(format!(
                "Le dossier {} est introuvable. Le disque est peut-être débranché.",
                loc.dir.display()
            ));
        }
        if !loc.db_path().is_file() {
            return Err(format!("Aucune base dans {}.", loc.dir.display()));
        }
    }
    let db = Db::open(&loc.dir).map_err(|e| e.to_string())?;
    relink_thumbnails(&db.conn.lock().unwrap(), &loc.thumbs_dir()).map_err(|e| e.to_string())?;
    Ok(db)
}

/// Les vignettes sont enregistrées par chemin absolu : après un déplacement, on les fait
/// pointer vers le dossier courant. Le nom du fichier, lui, ne change jamais.
fn relink_thumbnails(conn: &Connection, thumbs_dir: &Path) -> Result<(), rusqlite::Error> {
    let prefix = format!("{}/", thumbs_dir.display());
    conn.execute(
        "UPDATE videos SET thumbnail_path = ?1 || id || '.jpg'
         WHERE thumbnail_path IS NOT NULL AND thumbnail_path <> ?1 || id || '.jpg'",
        [prefix],
    )?;
    Ok(())
}

/// Copie la base et les vignettes dans `dest`, sans toucher à l'original.
///
/// La base passe par `VACUUM INTO` : une copie cohérente même en WAL, là où copier le
/// fichier laisserait derrière lui les écritures encore dans `-wal`.
pub fn copy_to(conn: &Connection, from: &DataLocation, dest: &Path) -> Result<(), DbError> {
    if !dest.is_dir() {
        return Err(DbError::Other(format!("dossier introuvable : {}", dest.display())));
    }
    if same_dir(dest, &from.dir) {
        return Err(DbError::Other("la base est déjà dans ce dossier".into()));
    }
    let target = dest.join(DB_FILE);
    if target.exists() {
        return Err(DbError::Other(format!(
            "{} contient déjà une base : l'ouvrir plutôt que la remplacer",
            dest.display()
        )));
    }

    conn.execute("VACUUM INTO ?1", [target.display().to_string()])?;

    let thumbs_from = from.thumbs_dir();
    let thumbs_to = dest.join(THUMBS_DIR);
    std::fs::create_dir_all(&thumbs_to)?;
    if thumbs_from.is_dir() {
        for entry in std::fs::read_dir(&thumbs_from)?.flatten() {
            if entry.path().is_file() {
                std::fs::copy(entry.path(), thumbs_to.join(entry.file_name()))?;
            }
        }
    }
    Ok(())
}

/// Retient `dir` comme emplacement. L'emplacement par défaut efface le choix.
pub fn remember(home: &Path, dir: &Path) -> Result<(), DbError> {
    let config = home.join(CONFIG_FILE);
    if same_dir(dir, home) {
        if config.exists() {
            std::fs::remove_file(config)?;
        }
        return Ok(());
    }
    std::fs::create_dir_all(home)?;
    let json = serde_json::to_string_pretty(&Config { data_dir: dir.to_path_buf() })
        .map_err(|e| DbError::Other(e.to_string()))?;
    std::fs::write(config, json)?;
    Ok(())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_at(home: &Path) -> DataLocation {
        resolve(home)
    }

    #[test]
    fn sans_choix_la_base_est_dans_le_dossier_de_donnees() {
        let home = tempfile::tempdir().unwrap();
        let loc = resolve(home.path());
        assert!(loc.is_default);
        assert_eq!(loc.dir, home.path());
        assert!(open(&loc).is_ok());
        assert!(loc.db_path().is_file());
    }

    #[test]
    fn un_dossier_choisi_mais_absent_n_ouvre_pas_de_base_neuve() {
        let home = tempfile::tempdir().unwrap();
        let ailleurs = home.path().join("disque-debranche");
        remember(home.path(), &ailleurs).unwrap();

        let loc = resolve(home.path());
        assert!(!loc.is_default);
        assert!(open(&loc).err().unwrap().contains("introuvable"));
        assert!(!ailleurs.exists(), "rien ne doit avoir été créé");
    }

    #[test]
    fn un_dossier_choisi_sans_base_n_en_cree_pas() {
        let home = tempfile::tempdir().unwrap();
        let vide = tempfile::tempdir().unwrap();
        remember(home.path(), vide.path()).unwrap();

        assert!(open(&resolve(home.path())).err().unwrap().contains("Aucune base"));
        assert!(!vide.path().join(DB_FILE).exists());
    }

    #[test]
    fn un_choix_illisible_n_ouvre_pas_l_emplacement_par_defaut() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(CONFIG_FILE), "pas du json").unwrap();

        let loc = resolve(home.path());
        assert!(open(&loc).err().unwrap().contains("illisible"));
        assert!(!loc.db_path().exists());
    }

    #[test]
    fn deplacer_copie_la_base_et_les_vignettes_puis_les_relie() {
        let home = tempfile::tempdir().unwrap();
        let from = default_at(home.path());
        let db = open(&from).unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO traps (id, name, created_at, updated_at)
                 VALUES ('t', 'Mare', 'x', 'x')",
                [],
            )
            .unwrap();
            std::fs::create_dir_all(from.thumbs_dir()).unwrap();
            let thumb = from.thumbs_dir().join("v1.jpg");
            std::fs::write(&thumb, b"jpg").unwrap();
            conn.execute(
                "INSERT INTO videos (id, trap_id, file_path, file_name, content_hash,
                   thumbnail_path, imported_at, created_at, updated_at)
                 VALUES ('v1', 't', '/v.mp4', 'v.mp4', 'h', ?1, 'x', 'x', 'x')",
                [thumb.display().to_string()],
            )
            .unwrap();
        }

        let dest = tempfile::tempdir().unwrap();
        copy_to(&db.conn.lock().unwrap(), &from, dest.path()).unwrap();
        remember(home.path(), dest.path()).unwrap();

        // L'original est intact.
        assert!(from.db_path().is_file());
        assert!(from.thumbs_dir().join("v1.jpg").is_file());

        let to = resolve(home.path());
        assert_eq!(to.dir, dest.path());
        let moved = open(&to).unwrap();
        let conn = moved.conn.lock().unwrap();
        let (name, thumb): (String, String) = conn
            .query_row(
                "SELECT t.name, v.thumbnail_path FROM videos v JOIN traps t ON t.id = v.trap_id",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "Mare");
        assert_eq!(PathBuf::from(&thumb), to.thumbs_dir().join("v1.jpg"));
        assert!(PathBuf::from(thumb).is_file());
    }

    #[test]
    fn deplacer_refuse_d_ecraser_une_base_existante() {
        let home = tempfile::tempdir().unwrap();
        let from = default_at(home.path());
        let db = open(&from).unwrap();
        let dest = tempfile::tempdir().unwrap();
        std::fs::write(dest.path().join(DB_FILE), b"autre base").unwrap();

        let err = copy_to(&db.conn.lock().unwrap(), &from, dest.path()).unwrap_err();
        assert!(err.to_string().contains("déjà une base"));
        assert_eq!(std::fs::read(dest.path().join(DB_FILE)).unwrap(), b"autre base");
    }

    #[test]
    fn revenir_a_l_emplacement_par_defaut_efface_le_choix() {
        let home = tempfile::tempdir().unwrap();
        let ailleurs = tempfile::tempdir().unwrap();
        remember(home.path(), ailleurs.path()).unwrap();
        remember(home.path(), home.path()).unwrap();
        assert!(resolve(home.path()).is_default);
    }
}

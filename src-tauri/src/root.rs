//! La racine des vidéos, vue de cette machine.
//!
//! La base peut servir à deux ordinateurs (dossier kDrive partagé) dont les chemins
//! diffèrent : `/Users/victor/kDrive/Pièges` sur le Mac, `/home/victor/kDrive/Pièges`
//! sous Linux. La base retient la racine sous laquelle ses chemins sont écrits
//! (`settings.root_path`) ; chaque machine retient la sienne dans `root.json`, à côté de
//! `location.json`, hors de la base. Quand elles diffèrent, les chemins de la base sont
//! réécrits vers la racine de la machine : seul le début change, la suite est la même
//! des deux côtés puisque kDrive reproduit l'arborescence.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::db::DbError;
use crate::settings;

const ROOT_FILE: &str = "root.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct LocalRoot {
    root_path: PathBuf,
}

/// Une réécriture des chemins, et ce qu'elle a retrouvé : on la montre, on ne la fait
/// pas en silence.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Rebase {
    pub from: String,
    pub to: String,
    /// Vidéos connues retrouvées sous la nouvelle racine.
    pub found: usize,
    /// Vidéos connues sous l'ancienne racine.
    pub total: usize,
}

/// La racine retenue par cette machine. Un fichier illisible est signalé au journal et
/// ignoré : rien n'est perdu, la racine de la base reprend la main ou se redemande.
pub fn local(home: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(home.join(ROOT_FILE)).ok()?;
    match serde_json::from_str::<LocalRoot>(&raw) {
        Ok(r) => Some(r.root_path),
        Err(e) => {
            log::warn!("{ROOT_FILE} illisible, ignoré : {e}");
            None
        }
    }
}

pub fn remember(home: &Path, root: &Path) -> Result<(), DbError> {
    std::fs::create_dir_all(home)?;
    let json = serde_json::to_string_pretty(&LocalRoot { root_path: root.to_path_buf() })
        .map_err(|e| DbError::Other(e.to_string()))?;
    std::fs::write(home.join(ROOT_FILE), json)?;
    Ok(())
}

/// Sans `/` final : `/a/b/` et `/a/b` sont la même racine, et la réécriture compare des
/// préfixes.
fn normalize(p: &str) -> String {
    let t = p.trim_end_matches('/');
    if t.is_empty() { "/".into() } else { t.to_string() }
}

/// Les chemins de vidéos encore présentes écrits sous `from`.
fn known_under(conn: &Connection, from: &str) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT file_path FROM videos
         WHERE deleted_at IS NULL AND file_state = 'present'
           AND substr(file_path, 1, length(?1) + 1) = ?1 || '/'",
    )?;
    let rows = stmt.query_map([from], |r| r.get(0))?;
    rows.collect()
}

/// Combien des vidéos connues sous `from` existent sous `to`.
fn probe(conn: &Connection, from: &str, to: &str) -> Result<(usize, usize), rusqlite::Error> {
    let paths = known_under(conn, from)?;
    let found = paths
        .iter()
        .filter(|p| Path::new(&format!("{to}{}", &p[from.len()..])).exists())
        .count();
    Ok((found, paths.len()))
}

/// Réécrit `from/…` en `to/…` partout où la base retient un chemin de vidéo, puis
/// retient `to` comme racine. Les vignettes ont leur propre rattachement (`location`).
/// L'historique des passes (`imports.root_path`) reste tel qu'il a eu lieu.
fn rebase(conn: &Connection, from: &str, to: &str) -> Result<(), DbError> {
    conn.execute_batch("BEGIN")?;
    let result = (|| -> Result<(), rusqlite::Error> {
        for table in ["videos", "purged_videos"] {
            conn.execute(
                &format!(
                    "UPDATE {table} SET file_path = ?2 || substr(file_path, length(?1) + 1)
                     WHERE substr(file_path, 1, length(?1) + 1) = ?1 || '/'"
                ),
                params![from, to],
            )?;
        }
        settings::set(conn, settings::ROOT_PATH, to)
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e.into())
        }
    }
}

/// À l'ouverture de la base : accorde la racine de la base et celle de cette machine.
///
/// - la racine de la base existe ici : c'est elle, cette machine la retient ;
/// - elle n'existe pas, mais celle de la machine oui : la base vient de l'autre
///   ordinateur, ses chemins sont réécrits — à condition d'y retrouver au moins une des
///   vidéos connues. Sinon ce n'est pas le même dossier et l'on ne touche à rien ;
///   l'interface signale la racine introuvable et propose de la désigner ;
/// - base sans racine : elle prend celle de la machine.
pub fn sync_on_open(conn: &Connection, home: &Path) -> Result<Option<Rebase>, DbError> {
    let db_root = settings::get(conn, settings::ROOT_PATH)?.map(|r| normalize(&r));
    let local = local(home).map(|p| normalize(&p.display().to_string()));

    match (db_root, local) {
        (Some(d), l) if Path::new(&d).is_dir() => {
            if l.as_deref() != Some(d.as_str()) {
                remember(home, Path::new(&d))?;
            }
            Ok(None)
        }
        (Some(d), Some(l)) if Path::new(&l).is_dir() => {
            let (found, total) = probe(conn, &d, &l)?;
            if total > 0 && found == 0 {
                log::warn!("racine {d} introuvable ici, et aucune vidéo connue sous {l} : rien réécrit");
                return Ok(None);
            }
            rebase(conn, &d, &l)?;
            log::info!("racine {d} → {l} : {found}/{total} vidéos retrouvées");
            Ok(Some(Rebase { from: d, to: l, found, total }))
        }
        (None, Some(l)) if Path::new(&l).is_dir() => {
            settings::set(conn, settings::ROOT_PATH, &l)?;
            Ok(None)
        }
        _ => Ok(None),
    }
}

/// Désigner, sur cette machine, le dossier qui correspond à la racine de la base.
/// Refusé si aucune vidéo connue ne s'y trouve : ce serait un autre dossier, et en
/// réécrivant les chemins on ferait passer tout l'index pour disparu.
pub fn relocate(conn: &Connection, home: &Path, to: &Path) -> Result<Rebase, DbError> {
    if !to.is_dir() {
        return Err(DbError::Other(format!("dossier introuvable : {}", to.display())));
    }
    let to = normalize(&to.display().to_string());
    let from = settings::get(conn, settings::ROOT_PATH)?
        .map(|r| normalize(&r))
        .ok_or_else(|| DbError::Other("aucune racine à retrouver : en choisir une".into()))?;
    let (found, total) = probe(conn, &from, &to)?;
    if total > 0 && found == 0 {
        return Err(DbError::Other(format!(
            "Aucune des {total} vidéos connues ne se trouve sous {to} : ce n'est sans doute \
             pas le même dossier. Pour indexer un autre dossier, utilise « Changer »."
        )));
    }
    rebase(conn, &from, &to)?;
    remember(home, Path::new(&to))?;
    Ok(Rebase { from, to, found, total })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO traps (id, name, created_at, updated_at) VALUES ('t', 'Mare', 'x', 'x')",
            [],
        )
        .unwrap();
        conn
    }

    fn video(conn: &Connection, id: &str, path: &str) {
        conn.execute(
            "INSERT INTO videos (id, trap_id, file_path, file_name, content_hash,
               imported_at, created_at, updated_at)
             VALUES (?1, 't', ?2, 'v.mp4', ?1, 'x', 'x', 'x')",
            params![id, path],
        )
        .unwrap();
    }

    fn path_of(conn: &Connection, id: &str) -> String {
        conn.query_row("SELECT file_path FROM videos WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    /// Une racine « ici » contenant `Mare/a.mp4`.
    fn here() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Mare")).unwrap();
        std::fs::write(dir.path().join("Mare/a.mp4"), b"").unwrap();
        dir
    }

    #[test]
    fn une_base_venue_de_l_autre_machine_est_reecrite_vers_la_racine_d_ici() {
        let conn = base();
        let home = tempfile::tempdir().unwrap();
        let root = here();
        let ici = root.path().display().to_string();
        settings::set(&conn, settings::ROOT_PATH, "/Users/victor/kDrive/Pièges").unwrap();
        video(&conn, "a", "/Users/victor/kDrive/Pièges/Mare/a.mp4");
        // Ailleurs que sous la racine : pas touché.
        video(&conn, "b", "/Volumes/Autre/b.mp4");
        conn.execute(
            "INSERT INTO purged_videos (content_hash, file_name, purged_at, file_path)
             VALUES ('p', 'p.mp4', 'x', '/Users/victor/kDrive/Pièges/Mare/p.mp4')",
            [],
        )
        .unwrap();
        remember(home.path(), root.path()).unwrap();

        let r = sync_on_open(&conn, home.path()).unwrap().unwrap();
        assert_eq!((r.found, r.total), (1, 1));
        assert_eq!(path_of(&conn, "a"), format!("{ici}/Mare/a.mp4"));
        assert_eq!(path_of(&conn, "b"), "/Volumes/Autre/b.mp4");
        let purged: String = conn
            .query_row("SELECT file_path FROM purged_videos", [], |r| r.get(0))
            .unwrap();
        assert_eq!(purged, format!("{ici}/Mare/p.mp4"));
        assert_eq!(settings::get(&conn, settings::ROOT_PATH).unwrap().unwrap(), ici);
    }

    #[test]
    fn une_racine_presente_ici_n_est_jamais_reecrite_et_devient_celle_de_la_machine() {
        let conn = base();
        let home = tempfile::tempdir().unwrap();
        let root = here();
        let autre = here();
        let ici = root.path().display().to_string();
        settings::set(&conn, settings::ROOT_PATH, &ici).unwrap();
        video(&conn, "a", &format!("{ici}/Mare/a.mp4"));
        remember(home.path(), autre.path()).unwrap();

        assert!(sync_on_open(&conn, home.path()).unwrap().is_none());
        assert_eq!(path_of(&conn, "a"), format!("{ici}/Mare/a.mp4"));
        assert_eq!(local(home.path()).unwrap(), root.path());
    }

    #[test]
    fn sans_aucune_video_retrouvee_on_ne_reecrit_rien() {
        let conn = base();
        let home = tempfile::tempdir().unwrap();
        let vide = tempfile::tempdir().unwrap();
        settings::set(&conn, settings::ROOT_PATH, "/Users/victor/kDrive/Pièges").unwrap();
        video(&conn, "a", "/Users/victor/kDrive/Pièges/Mare/a.mp4");
        remember(home.path(), vide.path()).unwrap();

        assert!(sync_on_open(&conn, home.path()).unwrap().is_none());
        assert_eq!(path_of(&conn, "a"), "/Users/victor/kDrive/Pièges/Mare/a.mp4");
    }

    #[test]
    fn un_prefixe_commun_n_est_pas_une_racine() {
        let conn = base();
        let home = tempfile::tempdir().unwrap();
        let root = here();
        settings::set(&conn, settings::ROOT_PATH, "/kDrive/Pièges").unwrap();
        video(&conn, "a", "/kDrive/Pièges/Mare/a.mp4");
        video(&conn, "b", "/kDrive/Pièges-bis/Mare/b.mp4");
        remember(home.path(), root.path()).unwrap();

        sync_on_open(&conn, home.path()).unwrap().unwrap();
        assert_eq!(path_of(&conn, "b"), "/kDrive/Pièges-bis/Mare/b.mp4");
    }

    #[test]
    fn designer_la_racine_ici_reecrit_les_chemins_et_la_retient() {
        let conn = base();
        let home = tempfile::tempdir().unwrap();
        let root = here();
        settings::set(&conn, settings::ROOT_PATH, "/Users/victor/kDrive/Pièges/").unwrap();
        video(&conn, "a", "/Users/victor/kDrive/Pièges/Mare/a.mp4");

        let r = relocate(&conn, home.path(), root.path()).unwrap();
        assert_eq!((r.found, r.total), (1, 1));
        assert_eq!(path_of(&conn, "a"), format!("{}/Mare/a.mp4", root.path().display()));
        assert_eq!(local(home.path()).unwrap(), root.path());
    }

    #[test]
    fn designer_un_dossier_sans_aucune_video_connue_est_refuse() {
        let conn = base();
        let home = tempfile::tempdir().unwrap();
        let vide = tempfile::tempdir().unwrap();
        settings::set(&conn, settings::ROOT_PATH, "/Users/victor/kDrive/Pièges").unwrap();
        video(&conn, "a", "/Users/victor/kDrive/Pièges/Mare/a.mp4");

        let err = relocate(&conn, home.path(), vide.path()).unwrap_err().to_string();
        assert!(err.contains("pas le même dossier"), "{err}");
        assert_eq!(path_of(&conn, "a"), "/Users/victor/kDrive/Pièges/Mare/a.mp4");
        assert!(local(home.path()).is_none());
    }

    #[test]
    fn une_base_sans_racine_prend_celle_de_la_machine() {
        let conn = base();
        let home = tempfile::tempdir().unwrap();
        let root = here();
        remember(home.path(), root.path()).unwrap();
        sync_on_open(&conn, home.path()).unwrap();
        assert_eq!(
            settings::get(&conn, settings::ROOT_PATH).unwrap().unwrap(),
            root.path().display().to_string()
        );
    }
}

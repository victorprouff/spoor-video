//! La pastille « des vidéos attendent ».
//!
//! Elle doit dire **exactement ce que l'indexation ferait**. Une première version se
//! contentait de comparer les chemins connus : elle annonçait donc comme nouvelle une
//! vidéo que la passe allait de toute façon écarter, et le message ne partait jamais.
//! Une pastille qu'on ne peut pas faire disparaître cesse d'être lue.
//!
//! Tout se fait sans lire les fichiers : ni empreinte, ni `ffprobe`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;

use crate::db::DbError;

#[derive(Debug, Default, serde::Serialize)]
pub struct PendingReport {
    /// Vidéos que la prochaine passe indexerait vraiment. C'est le seul chiffre qui
    /// mérite une alerte.
    pub new_files: usize,
    /// Fichiers posés dans un dossier non rattaché à un piège, ou à la racine. La passe
    /// les voit et les laisse : rien ne bougera tant qu'un piège ne les réclame pas.
    pub unlinked: usize,
    /// Noms des dossiers concernés, pour dire lesquels.
    pub unlinked_folders: Vec<String>,
    /// Fichiers écartés sans trace et revenus sur le disque — restaurés depuis la
    /// corbeille, ou resynchronisés par un client cloud. La passe les ré-ignore, par
    /// construction.
    pub discarded: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct DiscardedFile {
    pub content_hash: String,
    pub file_name: String,
    pub file_path: String,
    pub purged_at: String,
}

fn top_folder(root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    let mut components = relative.components();
    let first = components.next()?;
    components.next()?;
    Some(first.as_os_str().to_string_lossy().to_string()).filter(|n| !n.is_empty())
}

pub fn report(conn: &Connection, root: &Path) -> Result<PendingReport, DbError> {
    let mut out = PendingReport::default();
    if !root.is_dir() {
        return Ok(out);
    }

    let known: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT file_path FROM videos WHERE deleted_at IS NULL")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<_, _>>()?
    };

    let discarded: HashSet<String> = {
        let mut stmt =
            conn.prepare("SELECT file_path FROM purged_videos WHERE file_path IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<_, _>>()?
    };

    let linked: HashMap<String, ()> = {
        let mut stmt = conn.prepare(
            "SELECT folder_name FROM traps
             WHERE folder_name IS NOT NULL AND deleted_at IS NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, ())))?;
        rows.collect::<Result<_, _>>()?
    };

    let mut unlinked_folders: HashSet<String> = HashSet::new();

    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // La racine est exemptée : elle peut vivre dans un dossier commençant par
            // un point, et la filtrer viderait tout le décompte sans rien dire.
            e.depth() == 0
                || !e
                    .file_name()
                    .to_str()
                    .map(|n| n.starts_with('.') || n == "__MACOSX")
                    .unwrap_or(false)
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() || !crate::scan::is_video_path(entry.path()) {
            continue;
        }
        let path = entry.path().display().to_string();
        if known.contains(&path) {
            continue;
        }
        if discarded.contains(&path) {
            out.discarded += 1;
            continue;
        }
        match top_folder(root, entry.path()) {
            Some(folder) if linked.contains_key(&folder) => out.new_files += 1,
            Some(folder) => {
                out.unlinked += 1;
                unlinked_folders.insert(folder);
            }
            None => {
                out.unlinked += 1;
                unlinked_folders.insert("(racine)".into());
            }
        }
    }

    out.unlinked_folders = {
        let mut v: Vec<String> = unlinked_folders.into_iter().collect();
        v.sort();
        v
    };
    Ok(out)
}

/// Les vidéos écartées sans trace dont le fichier est de nouveau sur le disque.
pub fn discarded_files(conn: &Connection) -> Result<Vec<DiscardedFile>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT content_hash, file_name, file_path, purged_at FROM purged_videos
         WHERE file_path IS NOT NULL ORDER BY purged_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(DiscardedFile {
            content_hash: r.get(0)?,
            file_name: r.get(1)?,
            file_path: r.get(2)?,
            purged_at: r.get(3)?,
        })
    })?;
    let all: Vec<DiscardedFile> = rows.collect::<Result<_, _>>()?;
    Ok(all
        .into_iter()
        .filter(|f| Path::new(&f.file_path).exists())
        .collect())
}

/// Oublie le refus : la prochaine passe indexera de nouveau ce fichier.
///
/// Sans cette porte de sortie, une vidéo écartée par erreur resterait à jamais
/// inindexable, sans aucun moyen de revenir en arrière depuis l'application.
pub fn restore(conn: &Connection, content_hash: &str) -> Result<(), DbError> {
    let removed = conn.execute(
        "DELETE FROM purged_videos WHERE content_hash = ?1",
        [content_hash],
    )?;
    if removed == 0 {
        return Err(DbError::Other("cette vidéo n'est pas dans les écartées".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::params;

    struct Fixture {
        dir: tempfile::TempDir,
        conn: Connection,
    }

    fn db() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("VE81")).unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO traps (id, name, folder_name, created_at, updated_at)
             VALUES ('t1', 'VE81', 'VE81', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        Fixture { dir, conn }
    }

    fn file(f: &Fixture, relative: &str) -> String {
        let path = f.dir.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"x").unwrap();
        path.display().to_string()
    }

    #[test]
    fn compte_ce_que_la_passe_indexerait_vraiment() {
        let f = db();
        file(&f, "VE81/a.mp4");
        file(&f, "VE81/b.mp4");
        file(&f, "VE81/notes.txt");

        let r = report(&f.conn, f.dir.path()).unwrap();
        assert_eq!(r.new_files, 2, "le .txt n'est pas une vidéo");
        assert_eq!(r.unlinked, 0);
        assert_eq!(r.discarded, 0);
    }

    #[test]
    fn une_video_deja_indexee_ne_compte_pas() {
        let f = db();
        let path = file(&f, "VE81/a.mp4");
        f.conn
            .execute(
                "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id,
                     imported_at, created_at, updated_at)
                 VALUES ('v1', ?1, 'a.mp4', 'h1', 't1',
                         datetime('now'), datetime('now'), datetime('now'))",
                params![path],
            )
            .unwrap();
        assert_eq!(report(&f.conn, f.dir.path()).unwrap().new_files, 0);
    }

    #[test]
    fn une_video_ecartee_et_revenue_ne_reste_pas_en_attente() {
        let f = db();
        let path = file(&f, "VE81/DSCF0083.MP4");
        f.conn
            .execute(
                "INSERT INTO purged_videos (content_hash, file_name, purged_at, file_path)
                 VALUES ('h1', 'DSCF0083.MP4', datetime('now'), ?1)",
                params![path],
            )
            .unwrap();

        let r = report(&f.conn, f.dir.path()).unwrap();
        assert_eq!(
            r.new_files, 0,
            "la passe l'ignorera : l'annoncer comme nouvelle donnerait un message \
             qu'on ne peut jamais faire partir"
        );
        assert_eq!(r.discarded, 1, "mais elle est comptée, et nommable");
    }

    #[test]
    fn un_dossier_non_rattache_est_compte_a_part() {
        let f = db();
        file(&f, "Nouveau piege/a.mp4");
        file(&f, "posee-a-la-racine.mp4");

        let r = report(&f.conn, f.dir.path()).unwrap();
        assert_eq!(r.new_files, 0, "la passe n'en importera aucune");
        assert_eq!(r.unlinked, 2);
        assert_eq!(r.unlinked_folders, vec!["(racine)", "Nouveau piege"]);
    }

    #[test]
    fn les_ecartees_revenues_se_listent_et_se_reintegrent() {
        let f = db();
        let present = file(&f, "VE81/revenue.mp4");
        f.conn
            .execute(
                "INSERT INTO purged_videos (content_hash, file_name, purged_at, file_path)
                 VALUES ('h1', 'revenue.mp4', datetime('now'), ?1),
                        ('h2', 'partie.mp4', datetime('now'), '/nulle/part.mp4')",
                params![present],
            )
            .unwrap();

        let listed = discarded_files(&f.conn).unwrap();
        assert_eq!(listed.len(), 1, "seules celles réellement sur le disque");
        assert_eq!(listed[0].file_name, "revenue.mp4");

        restore(&f.conn, "h1").unwrap();
        assert_eq!(report(&f.conn, f.dir.path()).unwrap().new_files, 1);
        assert!(restore(&f.conn, "inconnue").is_err());
    }

    #[test]
    fn une_racine_absente_ne_fait_pas_echouer_la_pastille() {
        let f = db();
        let r = report(&f.conn, Path::new("/nulle/part")).unwrap();
        assert_eq!(r.new_files, 0, "un disque débranché ne casse pas l'accueil");
    }
}

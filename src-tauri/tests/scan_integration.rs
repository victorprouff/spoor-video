//! Test de bout en bout de la passe d'indexation (§4), sur de vraies vidéos.
//!
//! Les vidéos sont fabriquées par ffmpeg : sans fichier réel, on ne testerait que
//! nos propres suppositions sur ce que ffprobe rend.

use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::Connection;
use spoor_video_lib::testing::{apply_migrations, media_available, media_init, scan};

/// Fabrique une vidéo de `seconds` secondes, unique par sa couleur.
fn make_video(dest: &Path, seconds: f64, color: &str) {
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    let status = Command::new("ffmpeg")
        .args(["-v", "quiet", "-y", "-f", "lavfi", "-i"])
        .arg(format!("color=c={color}:s=320x240:d={seconds}:r=10"))
        .args(["-metadata", "creation_time=2026-03-14T21:05:00Z"])
        .arg(dest)
        .status()
        .expect("ffmpeg doit être disponible pour ce test");
    assert!(status.success(), "génération de la vidéo échouée");
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    thumbs: PathBuf,
    conn: Connection,
}

/// ffmpeg doit être résolu **avant** qu'on demande s'il est disponible : sans cet
/// appel, `media_available()` rend faux et chaque test sortirait en silence en
/// se donnant l'air de passer.
fn ffmpeg_ou_echec() {
    media_init(None);
    assert!(
        media_available(),
        "ffmpeg et ffprobe sont nécessaires à ces tests"
    );
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("videos");
    let thumbs = dir.path().join("thumbs");
    std::fs::create_dir_all(&root).unwrap();

    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    apply_migrations(&conn).unwrap();
    media_init(None);

    Fixture {
        _dir: dir,
        root,
        thumbs,
        conn,
    }
}

/// Rattache un dossier à un piège, comme le fait l'interface.
fn link(conn: &Connection, folder: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO traps (id, name, folder_name, created_at, updated_at)
         VALUES (?1, ?2, ?2, datetime('now'), datetime('now'))",
        rusqlite::params![id, folder],
    )
    .unwrap();
    id
}

#[test]
fn indexe_groupe_deduplique_et_suit_les_fichiers() {
    ffmpeg_ou_echec();
    let f = setup();

    // Un dossier de caméra rattaché, un autre pas.
    make_video(&f.root.join("Mare basse/a.mp4"), 1.0, "red");
    make_video(&f.root.join("Mare basse/DCIM/100MEDIA/b.mp4"), 1.0, "green");
    make_video(&f.root.join("Chablis nord/c.mp4"), 1.0, "blue");
    link(&f.conn, "Mare basse");

    // --- Première passe -----------------------------------------------------
    let r = scan(&f.conn, &f.root, &f.thumbs).unwrap();
    assert_eq!(r.files_seen, 3, "les trois fichiers sont vus");
    assert_eq!(r.files_added, 2, "seul le dossier rattaché est importé");
    assert_eq!(
        r.unknown_folders,
        vec!["Chablis nord".to_string()],
        "le dossier inconnu est signalé, pas importé en silence"
    );
    assert_eq!(r.files_error, 0);

    // Les sous-dossiers de la carte SD héritent bien du même piège.
    let traps_used: i64 = f
        .conn
        .query_row("SELECT COUNT(DISTINCT trap_id) FROM videos", [], |r| r.get(0))
        .unwrap();
    assert_eq!(traps_used, 1);

    // La date vient des métadonnées, et les vignettes existent.
    let (dated, thumbed): (i64, i64) = f
        .conn
        .query_row(
            "SELECT COUNT(recorded_at), COUNT(thumbnail_path) FROM videos",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(dated, 2, "la date de capture est lue");
    assert_eq!(thumbed, 2, "une vignette par vidéo");
    let thumb: String = f
        .conn
        .query_row("SELECT thumbnail_path FROM videos LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert!(Path::new(&thumb).exists(), "la vignette est sur le disque");

    // --- Rejouer la passe ne duplique rien ----------------------------------
    let r = scan(&f.conn, &f.root, &f.thumbs).unwrap();
    assert_eq!(r.files_added, 0, "une passe rejouée n'ajoute rien");
    assert_eq!(r.files_known, 2);
    let total: i64 = f
        .conn
        .query_row("SELECT COUNT(*) FROM videos", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 2);

    // --- Rattacher le dossier inconnu ---------------------------------------
    link(&f.conn, "Chablis nord");
    let r = scan(&f.conn, &f.root, &f.thumbs).unwrap();
    assert_eq!(r.files_added, 1, "le dossier rattaché est enfin importé");
    assert!(r.unknown_folders.is_empty());
}

#[test]
fn une_video_deplacee_est_retrouvee_pas_dupliquee() {
    ffmpeg_ou_echec();
    let f = setup();
    make_video(&f.root.join("Mare basse/a.mp4"), 1.0, "red");
    link(&f.conn, "Mare basse");
    scan(&f.conn, &f.root, &f.thumbs).unwrap();

    let id_avant: String = f
        .conn
        .query_row("SELECT id FROM videos", [], |r| r.get(0))
        .unwrap();

    // Renommée et rangée dans un sous-dossier, au sein du même piège.
    let nouveau = f.root.join("Mare basse/2026/mars/renommee.mp4");
    std::fs::create_dir_all(nouveau.parent().unwrap()).unwrap();
    std::fs::rename(f.root.join("Mare basse/a.mp4"), &nouveau).unwrap();

    let r = scan(&f.conn, &f.root, &f.thumbs).unwrap();
    assert_eq!(r.files_added, 0, "ce n'est pas une nouvelle vidéo");
    assert_eq!(r.files_moved, 1, "elle est reconnue à son empreinte");
    assert_eq!(r.files_missing, 0, "elle n'a pas disparu");

    let (id_apres, chemin, etat): (String, String, String) = f
        .conn
        .query_row(
            "SELECT id, file_path, file_state FROM videos",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(id_apres, id_avant, "la même ligne, donc les annotations tiennent");
    assert_eq!(chemin, nouveau.display().to_string());
    assert_eq!(etat, "present");
}

#[test]
fn un_fichier_disparu_passe_en_missing_puis_revient() {
    ffmpeg_ou_echec();
    let f = setup();
    let video = f.root.join("Mare basse/a.mp4");
    make_video(&video, 1.0, "red");
    link(&f.conn, "Mare basse");
    scan(&f.conn, &f.root, &f.thumbs).unwrap();

    // Supprimée à la main dans le Finder (§6 cas c).
    let sauvegarde = f._dir.path().join("sauvegarde.mp4");
    std::fs::rename(&video, &sauvegarde).unwrap();

    let r = scan(&f.conn, &f.root, &f.thumbs).unwrap();
    assert_eq!(r.files_missing, 1);
    let etat: String = f
        .conn
        .query_row("SELECT file_state FROM videos", [], |r| r.get(0))
        .unwrap();
    assert_eq!(etat, "missing", "la donnée reste, la vidéo est injouable");

    // Le disque est rebranché : le fichier revient, sous un autre nom.
    std::fs::rename(&sauvegarde, f.root.join("Mare basse/revenue.mp4")).unwrap();
    let r = scan(&f.conn, &f.root, &f.thumbs).unwrap();
    assert_eq!(r.files_recovered, 1, "reconnue à son empreinte");
    let etat: String = f
        .conn
        .query_row("SELECT file_state FROM videos", [], |r| r.get(0))
        .unwrap();
    assert_eq!(etat, "present", "débrancher un disque ne détruit rien");
}

#[test]
fn une_video_supprimee_sans_trace_ne_revient_pas() {
    ffmpeg_ou_echec();
    let f = setup();
    let video = f.root.join("Mare basse/fausse-declenche.mp4");
    make_video(&video, 1.0, "red");
    link(&f.conn, "Mare basse");
    scan(&f.conn, &f.root, &f.thumbs).unwrap();

    // §6 cas a : la ligne part, mais l'empreinte est mémorisée.
    let hash: String = f
        .conn
        .query_row("SELECT content_hash FROM videos", [], |r| r.get(0))
        .unwrap();
    f.conn.execute("DELETE FROM videos", []).unwrap();
    f.conn
        .execute(
            "INSERT INTO purged_videos (content_hash, file_name, purged_at)
             VALUES (?1, 'fausse-declenche.mp4', datetime('now'))",
            [&hash],
        )
        .unwrap();

    // La même carte SD repasse : la fausse déclenche ne doit pas revenir dans la file.
    let r = scan(&f.conn, &f.root, &f.thumbs).unwrap();
    assert_eq!(r.files_repurged, 1);
    assert_eq!(r.files_added, 0, "ce qui a été écarté une fois reste écarté");
    let total: i64 = f
        .conn
        .query_row("SELECT COUNT(*) FROM videos", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 0);
}

#[test]
fn une_video_sans_date_est_comptee_jamais_datee_du_jour() {
    ffmpeg_ou_echec();
    let f = setup();
    // Sans métadonnée de date : ffmpeg n'en écrit pas dans un .avi brut.
    let dest = f.root.join("Mare basse/sans-date.avi");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    Command::new("ffmpeg")
        .args([
            "-v", "quiet", "-y", "-f", "lavfi", "-i", "color=c=red:s=320x240:d=1:r=10",
        ])
        .args(["-c:v", "mjpeg"])
        .arg(&dest)
        .status()
        .unwrap();
    link(&f.conn, "Mare basse");

    let r = scan(&f.conn, &f.root, &f.thumbs).unwrap();
    assert_eq!(r.files_added, 1, "elle est bien indexée");
    assert_eq!(r.files_no_date, 1, "et comptée comme sans date");
    let date: Option<String> = f
        .conn
        .query_row("SELECT recorded_at FROM videos", [], |r| r.get(0))
        .unwrap();
    assert!(
        date.is_none(),
        "une date absente reste absente, jamais remplacée par aujourd'hui"
    );
}

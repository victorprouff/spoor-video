use rusqlite::{Connection, OptionalExtension};

/// Racine contenant un dossier par caméra (§4).
pub const ROOT_PATH: &str = "root_path";
/// Écart maximal, en minutes, entre deux vidéos d'une même séquence (§3).
pub const SEQUENCE_GAP_MINUTES: &str = "sequence_gap_minutes";

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .optional()
}

#[allow(dead_code)] // utilisé dès l'étape 2 (choix de la racine)
pub fn set(conn: &Connection, key: &str, value: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        [key, value],
    )?;
    Ok(())
}

#[allow(dead_code)]
pub fn sequence_gap_minutes(conn: &Connection) -> i64 {
    get(conn, SEQUENCE_GAP_MINUTES)
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
}

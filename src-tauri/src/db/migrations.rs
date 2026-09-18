use rusqlite::Connection;

use super::DbError;

/// Migrations numérotées, embarquées dans le binaire et appliquées au démarrage —
/// comme Spoor. Chaque migration tourne dans sa propre transaction : une migration
/// qui échoue ne laisse pas la base à moitié migrée.
///
/// Règle : on n'édite **jamais** une migration déjà livrée, on en ajoute une.
const MIGRATIONS: &[(i64, &str, &str)] = &[
    (
        1,
        "initial_schema",
        include_str!("../../migrations/001_initial_schema.sql"),
    ),
    (
        2,
        "seed_species",
        include_str!("../../migrations/002_seed_species.sql"),
    ),
];

pub fn apply(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            name       TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )?;

    let applied: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for (version, name, sql) in MIGRATIONS {
        if *version <= applied {
            continue;
        }
        log::info!("migration {version} — {name}");
        conn.execute_batch("BEGIN")?;
        match conn
            .execute_batch(sql)
            .and_then(|_| {
                conn.execute(
                    "INSERT INTO schema_migrations (version, name, applied_at)
                     VALUES (?1, ?2, datetime('now'))",
                    rusqlite::params![version, name],
                )
                .map(|_| ())
            }) {
            Ok(()) => {
                conn.execute_batch("COMMIT")?;
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(DbError::Other(format!(
                    "migration {version} ({name}) échouée : {e}"
                )));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply(&conn).unwrap();
        conn
    }

    #[test]
    fn applique_toutes_les_migrations() {
        let conn = migrated();
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, MIGRATIONS.last().unwrap().0);
    }

    #[test]
    fn rejouer_ne_change_rien() {
        let conn = migrated();
        apply(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);
        let species: i64 = conn
            .query_row("SELECT COUNT(*) FROM species", [], |r| r.get(0))
            .unwrap();
        assert!(species > 20, "les espèces ne doivent pas être dupliquées");
    }

    #[test]
    fn les_etats_de_fichier_sont_contraints() {
        let conn = migrated();
        conn.execute(
            "INSERT INTO traps (id, name, created_at, updated_at)
             VALUES ('t1', 'Test', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let bad = conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id,
                                 file_state, imported_at, created_at, updated_at)
             VALUES ('v1', '/a.mp4', 'a.mp4', 'h1', 't1', 'supprimee',
                     datetime('now'), datetime('now'), datetime('now'))",
            [],
        );
        assert!(bad.is_err(), "un file_state inconnu doit être refusé");
    }

    #[test]
    fn la_confiance_est_a_trois_niveaux() {
        let conn = migrated();
        conn.execute(
            "INSERT INTO traps (id, name, created_at, updated_at)
             VALUES ('t1', 'Test', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sequences (id, trap_id, started_at, ended_at, created_at, updated_at)
             VALUES ('s1', 't1', datetime('now'), datetime('now'), datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let species_id: String = conn
            .query_row("SELECT id FROM species LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let bad = conn.execute(
            "INSERT INTO sequence_species (sequence_id, species_id, confidence, created_at)
             VALUES ('s1', ?1, 'sur-de-sur', datetime('now'))",
            rusqlite::params![species_id],
        );
        assert!(bad.is_err(), "une note libre de confiance doit être refusée");
    }
}

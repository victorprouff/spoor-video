//! Le référentiel d'espèces : liste courte locale, extensible (§3).

use chrono::Utc;
use rusqlite::{params, Connection};

use crate::db::DbError;

#[derive(Debug, serde::Serialize)]
pub struct Species {
    pub id: String,
    pub common_name: String,
    pub scientific_name: Option<String>,
    pub species_group: String,
    pub color: Option<String>,
    pub sort_order: i64,
    pub shortcut_key: Option<String>,
    pub builtin: bool,
    /// Nombre de séquences annotées avec cette espèce : sert à refuser une suppression
    /// qui effacerait des identifications.
    pub usage_count: i64,
}

#[derive(Debug, serde::Deserialize)]
pub struct SpeciesInput {
    pub common_name: String,
    pub scientific_name: Option<String>,
    pub species_group: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i64>,
    pub shortcut_key: Option<String>,
}

const GROUPS: &[&str] = &["mammifere", "oiseau", "autre"];

fn normalise(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn list(conn: &Connection) -> Result<Vec<Species>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.common_name, s.scientific_name, s.species_group, s.color,
                s.sort_order, s.shortcut_key, s.builtin,
                (SELECT COUNT(*) FROM sequence_species ss WHERE ss.species_id = s.id)
         FROM species s
         WHERE s.deleted_at IS NULL
         ORDER BY s.species_group, s.sort_order, s.common_name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Species {
            id: r.get(0)?,
            common_name: r.get(1)?,
            scientific_name: r.get(2)?,
            species_group: r.get(3)?,
            color: r.get(4)?,
            sort_order: r.get(5)?,
            shortcut_key: r.get(6)?,
            builtin: r.get::<_, i64>(7)? != 0,
            usage_count: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn check(input: &SpeciesInput) -> Result<(), DbError> {
    if input.common_name.trim().is_empty() {
        return Err(DbError::Other("une espèce doit avoir un nom".into()));
    }
    if let Some(group) = &input.species_group {
        if !GROUPS.contains(&group.as_str()) {
            return Err(DbError::Other(format!("groupe inconnu : {group}")));
        }
    }
    if let Some(key) = normalise(input.shortcut_key.clone()) {
        // Un raccourci doit tenir en une touche, sinon il n'en est pas un.
        let mut chars = key.chars();
        let c = chars.next().unwrap();
        if chars.next().is_some() || !c.is_alphanumeric() {
            return Err(DbError::Other(
                "un raccourci est une seule lettre ou un seul chiffre".into(),
            ));
        }
    }
    Ok(())
}

/// Les raccourcis réservés au dépouillement : `0` écarte, `1`/`2`/`3` donnent la
/// confiance. Les laisser prendre par une espèce casserait le mode plein écran.
const RESERVED_KEYS: &[char] = &['0', '1', '2', '3'];

fn shortcut(input: &SpeciesInput) -> Result<Option<String>, DbError> {
    let Some(key) = normalise(input.shortcut_key.clone()) else {
        return Ok(None);
    };
    let key = key.to_lowercase();
    let c = key.chars().next().unwrap();
    if RESERVED_KEYS.contains(&c) {
        return Err(DbError::Other(format!(
            "la touche « {c} » est réservée au dépouillement"
        )));
    }
    Ok(Some(key))
}

pub fn create(conn: &Connection, input: SpeciesInput) -> Result<String, DbError> {
    check(&input)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO species (id, common_name, scientific_name, species_group, color,
             sort_order, shortcut_key, builtin, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8)",
        params![
            id,
            input.common_name.trim(),
            normalise(input.scientific_name.clone()),
            input.species_group.clone().unwrap_or_else(|| "autre".into()),
            normalise(input.color.clone()),
            input.sort_order.unwrap_or(1000),
            shortcut(&input)?,
            now,
        ],
    )
    .map_err(friendly)?;
    Ok(id)
}

pub fn update(conn: &Connection, id: &str, input: SpeciesInput) -> Result<(), DbError> {
    check(&input)?;
    let changed = conn
        .execute(
            "UPDATE species SET common_name = ?2, scientific_name = ?3, species_group = ?4,
                 color = ?5, sort_order = ?6, shortcut_key = ?7, updated_at = ?8
             WHERE id = ?1 AND deleted_at IS NULL",
            params![
                id,
                input.common_name.trim(),
                normalise(input.scientific_name.clone()),
                input.species_group.clone().unwrap_or_else(|| "autre".into()),
                normalise(input.color.clone()),
                input.sort_order.unwrap_or(1000),
                shortcut(&input)?,
                Utc::now().to_rfc3339(),
            ],
        )
        .map_err(friendly)?;
    if changed == 0 {
        return Err(DbError::Other("espèce introuvable".into()));
    }
    Ok(())
}

/// Suppression douce. **Refusée dès qu'une séquence porte cette espèce** : supprimer
/// une espèce utilisée effacerait des identifications déjà faites.
pub fn delete(conn: &Connection, id: &str) -> Result<(), DbError> {
    let used: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sequence_species WHERE species_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    if used > 0 {
        return Err(DbError::Other(format!(
            "{used} séquence(s) portent cette espèce : elle ne peut pas être supprimée"
        )));
    }
    conn.execute(
        "UPDATE species SET deleted_at = ?2, shortcut_key = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// Traduit les violations d'unicité en phrases lisibles : « UNIQUE constraint failed »
/// ne dit rien à qui saisit une espèce.
fn friendly(e: rusqlite::Error) -> DbError {
    let msg = e.to_string();
    if msg.contains("UNIQUE") && msg.contains("common_name") {
        return DbError::Other("une espèce porte déjà ce nom".into());
    }
    if msg.contains("UNIQUE") && msg.contains("shortcut") {
        return DbError::Other("ce raccourci est déjà pris par une autre espèce".into());
    }
    DbError::Sqlite(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        conn
    }

    fn input(name: &str) -> SpeciesInput {
        SpeciesInput {
            common_name: name.into(),
            scientific_name: None,
            species_group: None,
            color: None,
            sort_order: None,
            shortcut_key: None,
        }
    }

    #[test]
    fn la_liste_pre_remplie_est_la() {
        let conn = db();
        let all = list(&conn).unwrap();
        assert!(all.len() > 30);
        assert!(all.iter().all(|s| s.builtin));
    }

    #[test]
    fn cree_une_espece() {
        let conn = db();
        let mut i = input("Lynx boréal");
        i.scientific_name = Some("Lynx lynx".into());
        i.species_group = Some("mammifere".into());
        create(&conn, i).unwrap();
        let all = list(&conn).unwrap();
        let lynx = all.iter().find(|s| s.common_name == "Lynx boréal").unwrap();
        assert!(!lynx.builtin, "une espèce ajoutée n'est pas pré-remplie");
    }

    #[test]
    fn refuse_un_doublon_de_nom() {
        let conn = db();
        let err = create(&conn, input("Chevreuil")).unwrap_err();
        assert!(err.to_string().contains("porte déjà ce nom"), "{err}");
    }

    #[test]
    fn refuse_un_raccourci_deja_pris() {
        let conn = db();
        let mut i = input("Lynx boréal");
        // « c » est celui du chevreuil.
        i.shortcut_key = Some("c".into());
        let err = create(&conn, i).unwrap_err();
        assert!(err.to_string().contains("déjà pris"), "{err}");
    }

    #[test]
    fn refuse_les_touches_reservees_au_depouillement() {
        let conn = db();
        for key in ["0", "1", "2", "3"] {
            let mut i = input(&format!("Espèce {key}"));
            i.shortcut_key = Some(key.into());
            let err = create(&conn, i).unwrap_err();
            assert!(err.to_string().contains("réservée"), "{err}");
        }
    }

    #[test]
    fn refuse_un_raccourci_de_plusieurs_touches() {
        let conn = db();
        let mut i = input("Lynx boréal");
        i.shortcut_key = Some("ly".into());
        assert!(create(&conn, i).is_err());
    }

    #[test]
    fn refuse_de_supprimer_une_espece_utilisee() {
        let conn = db();
        conn.execute(
            "INSERT INTO traps (id, name, created_at, updated_at)
             VALUES ('t1', 'T', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sequences (id, trap_id, started_at, ended_at, created_at, updated_at)
             VALUES ('s1', 't1', datetime('now'), datetime('now'), datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let id = create(&conn, input("Lynx boréal")).unwrap();
        conn.execute(
            "INSERT INTO sequence_species (sequence_id, species_id, confidence, created_at)
             VALUES ('s1', ?1, 'certain', datetime('now'))",
            [&id],
        )
        .unwrap();

        assert!(
            delete(&conn, &id).is_err(),
            "supprimer effacerait une identification déjà faite"
        );
    }

    #[test]
    fn supprimer_libere_le_raccourci() {
        let conn = db();
        let mut i = input("Lynx boréal");
        i.shortcut_key = Some("y".into());
        let id = create(&conn, i).unwrap();
        delete(&conn, &id).unwrap();

        // Sans libération, l'index unique garderait la touche prise par une espèce
        // que plus personne ne voit.
        let mut j = input("Loutre des marais");
        j.shortcut_key = Some("y".into());
        assert!(create(&conn, j).is_ok());
    }
}

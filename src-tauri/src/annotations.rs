//! Annotation des séquences (§5).
//!
//! Deux axes distincts, jamais confondus :
//! - **état** : ce que montre la séquence quand ce n'est pas une espèce
//!   (`empty`, `human`, `vehicle`, `livestock`, `unidentified`) ;
//! - **espèces** : plusieurs par séquence, chacune avec **sa** confiance.
//!
//! Spoor a un troisième axe, les tags. Il a été repris ici par mimétisme puis retiré
//! (migration 006) : l'espèce, l'état et les notes couvraient déjà le besoin.

use chrono::Utc;
use rusqlite::{params, Connection};

use crate::db::DbError;

#[derive(Debug, serde::Deserialize)]
pub struct SpeciesPick {
    pub species_id: String,
    pub confidence: String,
    pub count_min: Option<i64>,
    pub count_max: Option<i64>,
}

/// Ce qu'on applique à une sélection. Tous les champs sont facultatifs : une annotation
/// en masse ne doit toucher que ce qui a été demandé, et laisser le reste intact.
#[derive(Debug, Default, serde::Deserialize)]
pub struct Annotation {
    pub state: Option<String>,
    #[serde(default)]
    pub add_species: Vec<SpeciesPick>,
    #[serde(default)]
    pub remove_species: Vec<String>,
    pub notes: Option<String>,
    /// `Some(false)` remet explicitement une séquence « à dépouiller ».
    pub reviewed: Option<bool>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct AnnotateReport {
    pub sequences_touched: usize,
    pub species_added: usize,
    pub species_removed: usize,
}

const CONFIDENCES: &[&str] = &["certain", "probable", "possible"];
const STATES: &[&str] = &[
    "species",
    "empty",
    "unidentified",
    "human",
    "vehicle",
    "livestock",
];

/// Applique une annotation à plusieurs séquences, **en une transaction** : une annotation
/// en masse qui échoue à mi-parcours laisserait une sélection à moitié traitée, dont on
/// ne saurait plus quoi croire.
pub fn annotate(
    conn: &mut Connection,
    sequence_ids: &[String],
    input: Annotation,
) -> Result<AnnotateReport, DbError> {
    if sequence_ids.is_empty() {
        return Err(DbError::Other("aucune séquence sélectionnée".into()));
    }
    if let Some(state) = &input.state {
        if !STATES.contains(&state.as_str()) {
            return Err(DbError::Other(format!("état inconnu : {state}")));
        }
    }
    for pick in &input.add_species {
        if !CONFIDENCES.contains(&pick.confidence.as_str()) {
            return Err(DbError::Other(format!(
                "confiance inconnue : {}",
                pick.confidence
            )));
        }
    }

    // Un état qui n'est pas « espèce » contredit les espèces déjà identifiées.
    // On refuse plutôt que d'effacer en silence : une identification est du travail.
    if let Some(state) = &input.state {
        if state != "species" {
            let conflicting = count_species(conn, sequence_ids)?;
            if conflicting > 0 && input.remove_species.is_empty() {
                return Err(DbError::Other(format!(
                    "{conflicting} identification(s) d'espèce dans la sélection : \
                     les retirer d'abord, ou choisir l'état « espèce »"
                )));
            }
        }
    }

    let tx = conn.transaction()?;
    let now = Utc::now().to_rfc3339();
    let mut report = AnnotateReport {
        sequences_touched: sequence_ids.len(),
        ..Default::default()
    };

    for sequence_id in sequence_ids {
        let exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM sequences WHERE id = ?1 AND deleted_at IS NULL",
            [sequence_id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(DbError::Other(format!(
                "séquence introuvable : {sequence_id}"
            )));
        }

        for species_id in &input.remove_species {
            report.species_removed += tx.execute(
                "DELETE FROM sequence_species WHERE sequence_id = ?1 AND species_id = ?2",
                params![sequence_id, species_id],
            )?;
        }

        for pick in &input.add_species {
            if pick.count_min.is_some()
                && pick.count_max.is_some()
                && pick.count_min > pick.count_max
            {
                return Err(DbError::Other(
                    "l'effectif minimal dépasse l'effectif maximal".into(),
                ));
            }
            // Réannoter la même espèce met à jour sa confiance plutôt que d'échouer :
            // corriger « possible » en « certain » est un geste courant.
            report.species_added += tx.execute(
                "INSERT INTO sequence_species (sequence_id, species_id, confidence,
                     count_min, count_max, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (sequence_id, species_id) DO UPDATE SET
                     confidence = excluded.confidence,
                     count_min = excluded.count_min,
                     count_max = excluded.count_max",
                params![
                    sequence_id,
                    pick.species_id,
                    pick.confidence,
                    pick.count_min,
                    pick.count_max,
                    now
                ],
            )?;
        }

        // Une espèce identifiée implique l'état « espèce » : le laisser à NULL ferait
        // apparaître comme non dépouillée une séquence qui porte une identification.
        let state = match (&input.state, input.add_species.is_empty()) {
            (Some(s), _) => Some(s.clone()),
            (None, false) => Some("species".to_string()),
            (None, true) => None,
        };
        if let Some(state) = state {
            tx.execute(
                "UPDATE sequences SET state = ?2, updated_at = ?3 WHERE id = ?1",
                params![sequence_id, state, now],
            )?;
        }

        if let Some(notes) = &input.notes {
            let notes = notes.trim();
            tx.execute(
                "UPDATE sequences SET notes = ?2, updated_at = ?3 WHERE id = ?1",
                params![
                    sequence_id,
                    if notes.is_empty() { None } else { Some(notes) },
                    now
                ],
            )?;
        }

        // Annoter, c'est dépouiller : toute décision marque la séquence comme vue,
        // sauf demande explicite du contraire.
        let reviewed = match input.reviewed {
            Some(true) => Some(Some(now.clone())),
            Some(false) => Some(None),
            None if has_decision(&input) => Some(Some(now.clone())),
            None => None,
        };
        if let Some(value) = reviewed {
            tx.execute(
                "UPDATE sequences SET reviewed_at = ?2, updated_at = ?3 WHERE id = ?1",
                params![sequence_id, value, now],
            )?;
        }

        // Une séquence annotée est gelée : le regroupement automatique ne doit jamais
        // la défaire (§4).
        tx.execute(
            "UPDATE sequences SET auto_grouped = 0 WHERE id = ?1",
            [sequence_id],
        )?;
    }

    tx.commit()?;
    Ok(report)
}

fn has_decision(input: &Annotation) -> bool {
    input.state.is_some()
        || !input.add_species.is_empty()
        || !input.remove_species.is_empty()
        || input.notes.is_some()
}

fn count_species(conn: &Connection, sequence_ids: &[String]) -> Result<i64, DbError> {
    let placeholders = vec!["?"; sequence_ids.len()].join(",");
    let sql = format!(
        "SELECT COUNT(*) FROM sequence_species WHERE sequence_id IN ({placeholders})"
    );
    let params: Vec<&dyn rusqlite::ToSql> = sequence_ids
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    Ok(conn.query_row(&sql, params.as_slice(), |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO traps (id, name, created_at, updated_at)
             VALUES ('t1', 'Mare basse', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        for id in ["s1", "s2", "s3"] {
            conn.execute(
                "INSERT INTO sequences (id, trap_id, started_at, ended_at, video_count,
                     auto_grouped, created_at, updated_at)
                 VALUES (?1, 't1', datetime('now'), datetime('now'), 1, 1,
                         datetime('now'), datetime('now'))",
                [id],
            )
            .unwrap();
        }
        conn
    }

    fn species_ids(conn: &Connection, n: usize) -> Vec<String> {
        let mut stmt = conn.prepare("SELECT id FROM species LIMIT ?1").unwrap();
        stmt.query_map([n], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn pick(id: &str, confidence: &str) -> SpeciesPick {
        SpeciesPick {
            species_id: id.into(),
            confidence: confidence.into(),
            count_min: None,
            count_max: None,
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn annote_plusieurs_sequences_d_un_coup() {
        let mut conn = db();
        let sp = species_ids(&conn, 1);
        let r = annotate(
            &mut conn,
            &ids(&["s1", "s2", "s3"]),
            Annotation {
                add_species: vec![pick(&sp[0], "certain")],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.sequences_touched, 3);
        assert_eq!(r.species_added, 3);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sequence_species", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn plusieurs_especes_chacune_avec_sa_confiance() {
        let mut conn = db();
        let sp = species_ids(&conn, 2);
        annotate(
            &mut conn,
            &ids(&["s1"]),
            Annotation {
                add_species: vec![pick(&sp[0], "certain"), pick(&sp[1], "possible")],
                ..Default::default()
            },
        )
        .unwrap();

        let mut stmt = conn
            .prepare("SELECT confidence FROM sequence_species WHERE sequence_id = 's1' ORDER BY confidence")
            .unwrap();
        let confidences: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(confidences, vec!["certain", "possible"]);
    }

    #[test]
    fn reannoter_corrige_la_confiance() {
        let mut conn = db();
        let sp = species_ids(&conn, 1);
        for confidence in ["possible", "certain"] {
            annotate(
                &mut conn,
                &ids(&["s1"]),
                Annotation {
                    add_species: vec![pick(&sp[0], confidence)],
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let (n, c): (i64, String) = conn
            .query_row(
                "SELECT COUNT(*), MAX(confidence) FROM sequence_species WHERE sequence_id = 's1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(n, 1, "pas de doublon");
        assert_eq!(c, "certain", "la confiance est corrigée");
    }

    #[test]
    fn annoter_marque_depouille_et_gele() {
        let mut conn = db();
        annotate(
            &mut conn,
            &ids(&["s1"]),
            Annotation {
                state: Some("empty".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let (reviewed, auto, state): (Option<String>, i64, String) = conn
            .query_row(
                "SELECT reviewed_at, auto_grouped, state FROM sequences WHERE id = 's1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!(reviewed.is_some(), "annoter, c'est dépouiller");
        assert_eq!(auto, 0, "et gèle la séquence contre un regroupement");
        assert_eq!(state, "empty");
    }

    #[test]
    fn une_espece_implique_l_etat_espece() {
        let mut conn = db();
        let sp = species_ids(&conn, 1);
        annotate(
            &mut conn,
            &ids(&["s1"]),
            Annotation {
                add_species: vec![pick(&sp[0], "certain")],
                ..Default::default()
            },
        )
        .unwrap();
        let state: String = conn
            .query_row("SELECT state FROM sequences WHERE id = 's1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(state, "species");
    }

    #[test]
    fn refuse_d_ecraser_une_identification_par_un_etat_contradictoire() {
        let mut conn = db();
        let sp = species_ids(&conn, 1);
        annotate(
            &mut conn,
            &ids(&["s1"]),
            Annotation {
                add_species: vec![pick(&sp[0], "certain")],
                ..Default::default()
            },
        )
        .unwrap();

        let err = annotate(
            &mut conn,
            &ids(&["s1"]),
            Annotation {
                state: Some("empty".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("identification"), "{err}");

        // En retirant explicitement l'espèce, l'opération passe.
        annotate(
            &mut conn,
            &ids(&["s1"]),
            Annotation {
                state: Some("empty".into()),
                remove_species: vec![sp[0].clone()],
                ..Default::default()
            },
        )
        .unwrap();
    }

    #[test]
    fn une_annotation_qui_echoue_ne_laisse_rien_a_moitie_fait() {
        let mut conn = db();
        let sp = species_ids(&conn, 1);
        let err = annotate(
            &mut conn,
            &ids(&["s1", "s2", "inexistante"]),
            Annotation {
                add_species: vec![pick(&sp[0], "certain")],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("introuvable"), "{err}");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sequence_species", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 0,
            "s1 et s2 ne doivent pas rester annotées après l'échec sur la troisième"
        );
    }

    #[test]
    fn refuse_une_confiance_inventee() {
        let mut conn = db();
        let sp = species_ids(&conn, 1);
        assert!(annotate(
            &mut conn,
            &ids(&["s1"]),
            Annotation {
                add_species: vec![pick(&sp[0], "sur-de-sur")],
                ..Default::default()
            },
        )
        .is_err());
    }

    #[test]
    fn peut_remettre_a_depouiller() {
        let mut conn = db();
        annotate(
            &mut conn,
            &ids(&["s1"]),
            Annotation {
                state: Some("empty".into()),
                ..Default::default()
            },
        )
        .unwrap();
        annotate(
            &mut conn,
            &ids(&["s1"]),
            Annotation {
                reviewed: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let reviewed: Option<String> = conn
            .query_row("SELECT reviewed_at FROM sequences WHERE id = 's1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(reviewed.is_none());
    }

    #[test]
    fn refuse_un_effectif_incoherent() {
        let mut conn = db();
        let sp = species_ids(&conn, 1);
        let mut p = pick(&sp[0], "certain");
        p.count_min = Some(5);
        p.count_max = Some(2);
        assert!(annotate(
            &mut conn,
            &ids(&["s1"]),
            Annotation {
                add_species: vec![p],
                ..Default::default()
            },
        )
        .is_err());
    }
}

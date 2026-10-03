//! Statistiques (§7).
//!
//! Elles portent sur **la sélection courante** : les mêmes filtres que la grille.
//! Analyser « les renards de décembre, toutes années » doit se faire en cochant, pas
//! en apprenant un second jeu de filtres.

use rusqlite::{Connection, ToSql};

use crate::db::DbError;
use crate::grid::{where_clause, GridFilter};

#[derive(Debug, serde::Serialize)]
pub struct Stats {
    pub total_sequences: i64,
    pub total_videos: i64,
    /// Séquences portant au moins une espèce identifiée.
    pub identified_sequences: i64,
    pub unreviewed_sequences: i64,
    /// Séquences sans position solaire : le piège n'a pas de coordonnées.
    pub without_position: i64,
    pub hours: Vec<HourBucket>,
    pub species_hours: Vec<SpeciesHour>,
    pub solar: Vec<SolarBucket>,
    pub rhythm: Vec<RhythmBucket>,
    pub months: Vec<MonthBucket>,
    pub traps: Vec<TrapStat>,
    pub species: Vec<SpeciesStat>,
}

#[derive(Debug, serde::Serialize)]
pub struct HourBucket {
    pub hour: i64,
    pub count: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct SpeciesHour {
    pub species_id: String,
    pub common_name: String,
    pub color: Option<String>,
    pub hour: i64,
    pub count: i64,
}

/// Une tranche de 30 minutes autour du coucher du soleil. `bucket` est le début de la
/// tranche, en minutes depuis le coucher (négatif avant).
#[derive(Debug, serde::Serialize)]
pub struct SolarBucket {
    pub bucket: i64,
    pub count: i64,
}

/// Séquences regroupées par heure murale, tranche solaire **et ensemble d'espèces**.
///
/// C'est ce qui permet de choisir les espèces du rythme d'activité côté interface sans
/// compter deux fois un passage où deux espèces figurent ensemble : additionner des
/// comptages par espèce le ferait.
#[derive(Debug, serde::Serialize)]
pub struct RhythmBucket {
    pub hour: i64,
    /// Même tranche que `SolarBucket::bucket` ; absente si le piège n'a pas de position.
    pub solar_bucket: Option<i64>,
    /// Espèces de la séquence, triées ; vide si aucune n'est identifiée.
    pub species_ids: Vec<String>,
    pub count: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct MonthBucket {
    pub month: i64,
    pub count: i64,
    /// Nombre d'années distinctes contribuant à ce mois. Un décembre nourri par quatre
    /// années ne se lit pas comme un décembre unique : sans ce chiffre, un mois très
    /// couvert paraît simplement plus fréquenté.
    pub years: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct TrapStat {
    pub trap_id: String,
    pub name: String,
    pub sequences: i64,
    pub videos: i64,
    /// Nombre d'espèces distinctes vues à ce piège.
    pub species_richness: i64,
    pub first_at: Option<String>,
    pub last_at: Option<String>,
    /// Jours entre la première et la dernière capture. **Ce n'est pas un effort de
    /// piégeage** : les interruptions (batterie vide, piège relevé) ne s'y voient pas.
    pub span_days: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct SpeciesStat {
    pub species_id: String,
    pub common_name: String,
    pub color: Option<String>,
    pub sequences: i64,
    pub videos: i64,
    /// Séquences où l'identification est certaine.
    pub certain: i64,
    pub traps: i64,
    pub first_at: Option<String>,
    pub last_at: Option<String>,
}

pub fn compute(conn: &Connection, filter: GridFilter) -> Result<Stats, DbError> {
    let (where_sql, params) = where_clause(&filter);
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let p = refs.as_slice();

    // Toutes les agrégations partent de la même sélection : on la matérialise une fois
    // en sous-requête plutôt que de recopier la clause partout.
    let selected = format!("(SELECT s.id FROM sequences s WHERE {where_sql})");

    let total_sequences: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM sequences s WHERE {where_sql}"),
        p,
        |r| r.get(0),
    )?;

    let total_videos: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM videos v
             WHERE v.deleted_at IS NULL AND v.sequence_id IN {selected}"
        ),
        p,
        |r| r.get(0),
    )?;

    let identified_sequences: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT ss.sequence_id) FROM sequence_species ss
             WHERE ss.sequence_id IN {selected}"
        ),
        p,
        |r| r.get(0),
    )?;

    let unreviewed_sequences: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM sequences s
             WHERE {where_sql} AND s.reviewed_at IS NULL"
        ),
        p,
        |r| r.get(0),
    )?;

    let without_position: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM sequences s
             WHERE {where_sql} AND s.sun_phase IS NULL"
        ),
        p,
        |r| r.get(0),
    )?;

    // --- Rythme d'activité, en heure murale au piège ------------------------
    let hours = {
        let mut stmt = conn.prepare(&format!(
            "SELECT CAST(strftime('%H', s.started_at) AS INTEGER) AS h, COUNT(*)
             FROM sequences s WHERE {where_sql}
             GROUP BY h ORDER BY h"
        ))?;
        let rows = stmt.query_map(p, |r| {
            Ok(HourBucket {
                hour: r.get(0)?,
                count: r.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let species_hours = {
        let mut stmt = conn.prepare(&format!(
            "SELECT sp.id, sp.common_name, sp.color,
                    CAST(strftime('%H', s.started_at) AS INTEGER) AS h, COUNT(*)
             FROM sequences s
             JOIN sequence_species ss ON ss.sequence_id = s.id
             JOIN species sp ON sp.id = ss.species_id
             WHERE {where_sql}
             GROUP BY sp.id, h
             ORDER BY sp.sort_order, sp.common_name, h"
        ))?;
        let rows = stmt.query_map(p, |r| {
            Ok(SpeciesHour {
                species_id: r.get(0)?,
                common_name: r.get(1)?,
                color: r.get(2)?,
                hour: r.get(3)?,
                count: r.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    // --- Rythme d'activité, rapporté au coucher du soleil -------------------
    // Tranches de 30 minutes. C'est la lecture qui a du sens pour un animal : un renard
    // sort « au crépuscule », pas « à 18 h ». En heure civile, la même habitude semble
    // se déplacer de plusieurs heures entre juin et décembre.
    let solar = {
        let mut stmt = conn.prepare(&format!(
            "SELECT CAST(s.minutes_from_sunset / 30 AS INTEGER) * 30 AS bucket, COUNT(*)
             FROM sequences s
             WHERE {where_sql} AND s.minutes_from_sunset IS NOT NULL
             GROUP BY bucket ORDER BY bucket"
        ))?;
        let rows = stmt.query_map(p, |r| {
            Ok(SolarBucket {
                bucket: r.get(0)?,
                count: r.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let rhythm = {
        let mut stmt = conn.prepare(&format!(
            "SELECT h, bucket, ids, COUNT(*) FROM (
                SELECT CAST(strftime('%H', s.started_at) AS INTEGER) AS h,
                       CAST(s.minutes_from_sunset / 30 AS INTEGER) * 30 AS bucket,
                       (SELECT group_concat(species_id, ',') FROM (
                            SELECT ss.species_id FROM sequence_species ss
                            WHERE ss.sequence_id = s.id ORDER BY ss.species_id
                       )) AS ids
                FROM sequences s WHERE {where_sql}
             )
             GROUP BY h, bucket, ids"
        ))?;
        let rows = stmt.query_map(p, |r| {
            let ids: Option<String> = r.get(2)?;
            Ok(RhythmBucket {
                hour: r.get(0)?,
                solar_bucket: r.get(1)?,
                species_ids: ids
                    .map(|i| i.split(',').map(String::from).collect())
                    .unwrap_or_default(),
                count: r.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    // --- Saisonnalité : mois toutes années confondues -----------------------
    let months = {
        let mut stmt = conn.prepare(&format!(
            "SELECT CAST(strftime('%m', s.started_at) AS INTEGER) AS m, COUNT(*),
                    COUNT(DISTINCT strftime('%Y', s.started_at))
             FROM sequences s WHERE {where_sql}
             GROUP BY m ORDER BY m"
        ))?;
        let rows = stmt.query_map(p, |r| {
            Ok(MonthBucket {
                month: r.get(0)?,
                count: r.get(1)?,
                years: r.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    // --- Comparaison entre emplacements -------------------------------------
    // Deux requêtes plutôt qu'une avec sous-requêtes corrélées : chacune ne prend la
    // clause qu'une fois, donc les paramètres non plus. Une version « astucieuse » les
    // répétait trois fois — une source d'erreur pour un gain nul.
    let richness: std::collections::HashMap<String, i64> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT s.trap_id, COUNT(DISTINCT ss.species_id)
             FROM sequences s JOIN sequence_species ss ON ss.sequence_id = s.id
             WHERE {where_sql} GROUP BY s.trap_id"
        ))?;
        let rows = stmt.query_map(p, |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };

    let traps = {
        let mut stmt = conn.prepare(&format!(
            "SELECT t.id, t.name, COUNT(*), COALESCE(SUM(s.video_count), 0),
                    MIN(s.started_at), MAX(s.started_at),
                    CAST(julianday(MAX(s.started_at)) - julianday(MIN(s.started_at)) AS INTEGER)
             FROM sequences s JOIN traps t ON t.id = s.trap_id
             WHERE {where_sql}
             GROUP BY t.id ORDER BY COUNT(*) DESC"
        ))?;
        let rows = stmt.query_map(p, |r| {
            let trap_id: String = r.get(0)?;
            Ok(TrapStat {
                species_richness: *richness.get(&trap_id).unwrap_or(&0),
                trap_id,
                name: r.get(1)?,
                sequences: r.get(2)?,
                videos: r.get(3)?,
                first_at: r.get(4)?,
                last_at: r.get(5)?,
                span_days: r.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    // --- Par espèce ----------------------------------------------------------
    let species = {
        let mut stmt = conn.prepare(&format!(
            "SELECT sp.id, sp.common_name, sp.color,
                    COUNT(DISTINCT s.id),
                    COALESCE(SUM(s.video_count), 0),
                    SUM(CASE WHEN ss.confidence = 'certain' THEN 1 ELSE 0 END),
                    COUNT(DISTINCT s.trap_id),
                    MIN(s.started_at), MAX(s.started_at)
             FROM sequences s
             JOIN sequence_species ss ON ss.sequence_id = s.id
             JOIN species sp ON sp.id = ss.species_id
             WHERE {where_sql}
             GROUP BY sp.id ORDER BY COUNT(DISTINCT s.id) DESC"
        ))?;
        let rows = stmt.query_map(p, |r| {
            Ok(SpeciesStat {
                species_id: r.get(0)?,
                common_name: r.get(1)?,
                color: r.get(2)?,
                sequences: r.get(3)?,
                videos: r.get(4)?,
                certain: r.get(5)?,
                traps: r.get(6)?,
                first_at: r.get(7)?,
                last_at: r.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    Ok(Stats {
        total_sequences,
        total_videos,
        identified_sequences,
        unreviewed_sequences,
        without_position,
        hours,
        species_hours,
        solar,
        rhythm,
        months,
        traps,
        species,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{annotate, Annotation, SpeciesPick};
    use crate::db::migrations;
    use rusqlite::params;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO traps (id, name, latitude, longitude, created_at, updated_at)
             VALUES ('t1', 'Mare basse', 47.32, 5.04, datetime('now'), datetime('now')),
                    ('t2', 'Chablis nord', 47.30, 5.00, datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn
    }

    fn seq(conn: &Connection, id: &str, trap: &str, started: &str, videos: i64) {
        conn.execute(
            "INSERT INTO sequences (id, trap_id, started_at, ended_at, video_count,
                 auto_grouped, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3, ?4, 1, datetime('now'), datetime('now'))",
            params![id, trap, started, videos],
        )
        .unwrap();
    }

    fn species_id(conn: &Connection, n: usize) -> String {
        let mut stmt = conn.prepare("SELECT id FROM species LIMIT ?1").unwrap();
        let ids: Vec<String> = stmt
            .query_map([n + 1], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        ids[n].clone()
    }

    fn tag(conn: &mut Connection, seq_id: &str, sp: &str, confidence: &str) {
        annotate(
            conn,
            &[seq_id.to_string()],
            Annotation {
                add_species: vec![SpeciesPick {
                    species_id: sp.into(),
                    confidence: confidence.into(),
                    count_min: None,
                    count_max: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();
    }

    #[test]
    fn compte_la_selection() {
        let conn = db();
        seq(&conn, "s1", "t1", "2026-03-14T21:00:00", 3);
        seq(&conn, "s2", "t2", "2026-03-15T02:00:00", 1);

        let s = compute(&conn, GridFilter::default()).unwrap();
        assert_eq!(s.total_sequences, 2);
        assert_eq!(s.unreviewed_sequences, 2);
        assert_eq!(s.identified_sequences, 0);
    }

    #[test]
    fn distribue_par_heure_murale() {
        let conn = db();
        seq(&conn, "s1", "t1", "2026-03-14T21:30:00", 1);
        seq(&conn, "s2", "t1", "2026-03-14T21:45:00", 1);
        seq(&conn, "s3", "t1", "2026-03-15T02:00:00", 1);

        let s = compute(&conn, GridFilter::default()).unwrap();
        let at = |h: i64| s.hours.iter().find(|b| b.hour == h).map(|b| b.count);
        assert_eq!(at(21), Some(2));
        assert_eq!(at(2), Some(1));
        assert_eq!(at(13), None, "une heure sans passage n'invente pas de zéro");
    }

    #[test]
    fn la_saisonnalite_agrege_les_annees_et_les_compte() {
        let conn = db();
        // Trois décembres, deux années.
        seq(&conn, "s1", "t1", "2024-12-20T22:00:00", 1);
        seq(&conn, "s2", "t1", "2026-12-02T03:00:00", 1);
        seq(&conn, "s3", "t1", "2026-12-15T23:00:00", 1);
        seq(&conn, "s4", "t1", "2026-06-10T12:00:00", 1);

        let s = compute(&conn, GridFilter::default()).unwrap();
        let dec = s.months.iter().find(|m| m.month == 12).unwrap();
        assert_eq!(dec.count, 3);
        assert_eq!(
            dec.years, 2,
            "un décembre nourri par deux années ne se lit pas comme un décembre unique"
        );
        assert_eq!(s.months.iter().find(|m| m.month == 6).unwrap().years, 1);
    }

    #[test]
    fn compare_les_emplacements() {
        let mut conn = db();
        seq(&conn, "s1", "t1", "2026-03-01T21:00:00", 2);
        seq(&conn, "s2", "t1", "2026-03-20T21:00:00", 1);
        seq(&conn, "s3", "t2", "2026-03-10T21:00:00", 1);

        let (a, b) = (species_id(&conn, 0), species_id(&conn, 1));
        tag(&mut conn, "s1", &a, "certain");
        tag(&mut conn, "s2", &b, "certain");
        tag(&mut conn, "s3", &a, "certain");

        let s = compute(&conn, GridFilter::default()).unwrap();
        let mare = s.traps.iter().find(|t| t.trap_id == "t1").unwrap();
        assert_eq!(mare.sequences, 2);
        assert_eq!(mare.videos, 3);
        assert_eq!(mare.species_richness, 2);
        assert_eq!(mare.span_days, Some(19));

        let chablis = s.traps.iter().find(|t| t.trap_id == "t2").unwrap();
        assert_eq!(chablis.species_richness, 1);
    }

    #[test]
    fn compte_par_espece_avec_la_confiance() {
        let mut conn = db();
        seq(&conn, "s1", "t1", "2026-03-01T21:00:00", 2);
        seq(&conn, "s2", "t2", "2026-03-02T21:00:00", 3);
        let a = species_id(&conn, 0);
        tag(&mut conn, "s1", &a, "certain");
        tag(&mut conn, "s2", &a, "possible");

        let s = compute(&conn, GridFilter::default()).unwrap();
        let stat = s.species.iter().find(|x| x.species_id == a).unwrap();
        assert_eq!(stat.sequences, 2);
        assert_eq!(stat.videos, 5, "les déclenchements comptent, pas que les passages");
        assert_eq!(stat.certain, 1);
        assert_eq!(stat.traps, 2);
    }

    #[test]
    fn les_filtres_s_appliquent_aux_statistiques() {
        let conn = db();
        seq(&conn, "s1", "t1", "2026-12-20T22:00:00", 1);
        seq(&conn, "s2", "t1", "2026-06-10T12:00:00", 1);

        let hiver = compute(
            &conn,
            GridFilter {
                months: vec![12],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(hiver.total_sequences, 1);
        assert_eq!(hiver.months.len(), 1);
    }

    #[test]
    fn le_rythme_solaire_se_lit_autour_du_coucher() {
        let conn = db();
        seq(&conn, "s1", "t1", "2026-06-21T22:50:00", 1);
        seq(&conn, "s2", "t1", "2026-12-21T17:50:00", 1);
        conn.execute("UPDATE traps SET utc_offset_minutes = 120 WHERE id = 't1'", [])
            .unwrap();
        crate::sequences::refresh_sun(&conn).unwrap();

        let s = compute(&conn, GridFilter::default()).unwrap();
        assert_eq!(s.without_position, 0);
        // Les deux sorties ont lieu à peu près une heure après le coucher : en heure
        // solaire elles tombent dans des tranches voisines, là où l'heure civile les
        // séparerait de cinq heures.
        let buckets: Vec<i64> = s.solar.iter().map(|b| b.bucket).collect();
        assert!(
            buckets.iter().all(|b| (0..=180).contains(b)),
            "attendu des tranches juste après le coucher, obtenu {buckets:?}"
        );
    }

    #[test]
    fn une_sequence_sans_position_est_comptee_pas_cachee() {
        let conn = db();
        conn.execute("UPDATE traps SET latitude = NULL, longitude = NULL", [])
            .unwrap();
        seq(&conn, "s1", "t1", "2026-03-14T21:00:00", 1);

        let s = compute(&conn, GridFilter::default()).unwrap();
        assert_eq!(s.total_sequences, 1);
        assert_eq!(s.without_position, 1);
        assert!(s.solar.is_empty(), "sans position, aucun rythme solaire inventé");
    }

    #[test]
    fn le_rythme_ne_compte_pas_deux_fois_un_passage_a_deux_especes() {
        let mut conn = db();
        seq(&conn, "s1", "t1", "2026-03-01T21:10:00", 1);
        seq(&conn, "s2", "t1", "2026-03-02T21:40:00", 1);
        seq(&conn, "s3", "t1", "2026-03-03T03:00:00", 1);
        let (a, b) = (species_id(&conn, 0), species_id(&conn, 1));
        tag(&mut conn, "s1", &a, "certain");
        tag(&mut conn, "s1", &b, "certain");
        tag(&mut conn, "s2", &a, "certain");

        let s = compute(&conn, GridFilter::default()).unwrap();
        let total: i64 = s.rhythm.iter().map(|r| r.count).sum();
        assert_eq!(total, 3, "chaque passage une seule fois, espèces ou pas");

        let a_et_b = s.rhythm.iter().find(|r| r.species_ids.len() == 2).unwrap();
        assert_eq!(a_et_b.count, 1);
        assert_eq!(a_et_b.hour, 21);
        let mut attendu = vec![a.clone(), b.clone()];
        attendu.sort();
        assert_eq!(a_et_b.species_ids, attendu);

        let sans_espece = s.rhythm.iter().find(|r| r.species_ids.is_empty()).unwrap();
        assert_eq!(sans_espece.hour, 3, "un passage sans espèce reste dans le rythme");
    }
}

//! Exports (§7) : CSV des séquences et des détections, copie des fichiers.

use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, ToSql};

use crate::db::DbError;
use crate::grid::{where_clause, GridFilter};
use crate::traps::EFFECTIVE_RECORDED_AT;

/// Séparateur point-virgule et BOM UTF-8 : c'est ce qu'attend Excel en configuration
/// française, où la virgule est le séparateur décimal. Sans le BOM, les accents
/// arrivent illisibles ; avec la virgule, chaque ligne finit dans une seule colonne.
const SEPARATOR: char = ';';
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Échappe un champ : guillemets doublés, champ encadré s'il contient un séparateur,
/// un guillemet ou un retour à la ligne.
fn field(value: &str) -> String {
    if value.contains(SEPARATOR) || value.contains('"') || value.contains('\n') || value.contains('\r')
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn line(values: &[String]) -> String {
    let cells: Vec<String> = values.iter().map(|v| field(v)).collect();
    format!("{}\r\n", cells.join(&SEPARATOR.to_string()))
}

fn opt(value: Option<String>) -> String {
    value.unwrap_or_default()
}

#[derive(Debug, serde::Serialize)]
pub struct ExportReport {
    pub path: String,
    pub rows: usize,
}

/// Une ligne par séquence : la vue « un passage, une ligne ».
pub fn sequences_csv(
    conn: &Connection,
    filter: GridFilter,
    dest: &Path,
) -> Result<ExportReport, DbError> {
    let (where_sql, params) = where_clause(&filter);
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let sql = format!(
        "SELECT s.id, t.name, t.latitude, t.longitude,
                s.started_at, s.ended_at,
                CAST((julianday(s.ended_at) - julianday(s.started_at)) * 86400 AS INTEGER),
                s.video_count, s.state, s.sun_phase, s.minutes_from_sunset,
                (SELECT GROUP_CONCAT(sp.common_name, ' | ')
                   FROM sequence_species ss JOIN species sp ON sp.id = ss.species_id
                  WHERE ss.sequence_id = s.id),
                (SELECT GROUP_CONCAT(ss.confidence, ' | ')
                   FROM sequence_species ss JOIN species sp ON sp.id = ss.species_id
                  WHERE ss.sequence_id = s.id),
                (SELECT GROUP_CONCAT(tg.name, ' | ')
                   FROM sequence_tags st JOIN tags tg ON tg.id = st.tag_id
                  WHERE st.sequence_id = s.id),
                s.notes, s.reviewed_at
         FROM sequences s JOIN traps t ON t.id = s.trap_id
         WHERE {where_sql}
         ORDER BY s.started_at"
    );

    let mut out = std::fs::File::create(dest)?;
    out.write_all(BOM)?;
    out.write_all(
        line(&[
            "id_sequence".into(),
            "piege".into(),
            "latitude".into(),
            "longitude".into(),
            "debut".into(),
            "fin".into(),
            "duree_s".into(),
            "declenchements".into(),
            "etat".into(),
            "phase_solaire".into(),
            "minutes_apres_coucher".into(),
            "especes".into(),
            "confiances".into(),
            "tags".into(),
            "notes".into(),
            "depouille_le".into(),
        ])
        .as_bytes(),
    )?;

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(refs.as_slice())?;
    let mut count = 0;
    while let Some(r) = rows.next()? {
        out.write_all(
            line(&[
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                opt(r.get::<_, Option<f64>>(2)?.map(|v| format!("{v:.6}"))),
                opt(r.get::<_, Option<f64>>(3)?.map(|v| format!("{v:.6}"))),
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<i64>>(6)?.unwrap_or(0).to_string(),
                r.get::<_, i64>(7)?.to_string(),
                opt(r.get(8)?),
                opt(r.get(9)?),
                opt(r.get::<_, Option<i64>>(10)?.map(|v| v.to_string())),
                opt(r.get(11)?),
                opt(r.get(12)?),
                opt(r.get(13)?),
                opt(r.get(14)?),
                opt(r.get(15)?),
            ])
            .as_bytes(),
        )?;
        count += 1;
    }
    out.flush()?;

    Ok(ExportReport {
        path: dest.display().to_string(),
        rows: count,
    })
}

/// Une ligne par **détection** — une espèce dans une séquence.
///
/// C'est la forme à plat qu'attendent R et Python : une observation par ligne, aucun
/// champ à redécouper. Le CSV des séquences agrège les espèces dans une seule cellule,
/// pratique à lire mais pénible à analyser.
pub fn detections_csv(
    conn: &Connection,
    filter: GridFilter,
    dest: &Path,
) -> Result<ExportReport, DbError> {
    let (where_sql, params) = where_clause(&filter);
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let sql = format!(
        "SELECT s.id, t.name, t.latitude, t.longitude,
                s.started_at,
                CAST(strftime('%Y', s.started_at) AS INTEGER),
                CAST(strftime('%m', s.started_at) AS INTEGER),
                CAST(strftime('%H', s.started_at) AS INTEGER),
                sp.common_name, sp.scientific_name, ss.confidence,
                ss.count_min, ss.count_max,
                s.video_count,
                CAST((julianday(s.ended_at) - julianday(s.started_at)) * 86400 AS INTEGER),
                s.sun_phase, s.minutes_from_sunset
         FROM sequences s
         JOIN traps t ON t.id = s.trap_id
         JOIN sequence_species ss ON ss.sequence_id = s.id
         JOIN species sp ON sp.id = ss.species_id
         WHERE {where_sql}
         ORDER BY s.started_at, sp.common_name"
    );

    let mut out = std::fs::File::create(dest)?;
    out.write_all(BOM)?;
    out.write_all(
        line(&[
            "id_sequence".into(),
            "piege".into(),
            "latitude".into(),
            "longitude".into(),
            "debut".into(),
            "annee".into(),
            "mois".into(),
            "heure".into(),
            "espece".into(),
            "nom_scientifique".into(),
            "confiance".into(),
            "effectif_min".into(),
            "effectif_max".into(),
            "declenchements".into(),
            "duree_s".into(),
            "phase_solaire".into(),
            "minutes_apres_coucher".into(),
        ])
        .as_bytes(),
    )?;

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(refs.as_slice())?;
    let mut count = 0;
    while let Some(r) = rows.next()? {
        out.write_all(
            line(&[
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                opt(r.get::<_, Option<f64>>(2)?.map(|v| format!("{v:.6}"))),
                opt(r.get::<_, Option<f64>>(3)?.map(|v| format!("{v:.6}"))),
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?.to_string(),
                r.get::<_, i64>(6)?.to_string(),
                r.get::<_, i64>(7)?.to_string(),
                r.get::<_, String>(8)?,
                opt(r.get(9)?),
                r.get::<_, String>(10)?,
                opt(r.get::<_, Option<i64>>(11)?.map(|v| v.to_string())),
                opt(r.get::<_, Option<i64>>(12)?.map(|v| v.to_string())),
                r.get::<_, i64>(13)?.to_string(),
                r.get::<_, Option<i64>>(14)?.unwrap_or(0).to_string(),
                opt(r.get(15)?),
                opt(r.get::<_, Option<i64>>(16)?.map(|v| v.to_string())),
            ])
            .as_bytes(),
        )?;
        count += 1;
    }
    out.flush()?;

    Ok(ExportReport {
        path: dest.display().to_string(),
        rows: count,
    })
}

#[derive(Debug, Default, serde::Serialize)]
pub struct CopyReport {
    pub dest: String,
    pub copied: usize,
    /// Fichiers dont la vidéo n'existe plus (`purged` ou `missing`) : la donnée reste,
    /// le fichier non.
    pub unavailable: usize,
    pub errors: Vec<String>,
    pub bytes: u64,
}

/// Remplace ce qui ne peut pas figurer dans un nom de fichier, et borne la longueur.
fn safe(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    cleaned.trim().chars().take(60).collect()
}

/// Copie les vidéos des séquences choisies vers un dossier.
///
/// **Copie, jamais déplacement** : l'original reste indexé et à sa place. Un fichier
/// déplacé passerait en `missing` à la passe suivante, et l'application perdrait la
/// vidéo qu'elle vient elle-même de ranger ailleurs.
pub fn copy_videos(
    conn: &Connection,
    sequence_ids: &[String],
    dest_dir: &Path,
) -> Result<CopyReport, DbError> {
    if sequence_ids.is_empty() {
        return Err(DbError::Other("aucune séquence sélectionnée".into()));
    }
    if !dest_dir.is_dir() {
        return Err(DbError::Other(format!(
            "dossier introuvable : {}",
            dest_dir.display()
        )));
    }

    let holes = vec!["?"; sequence_ids.len()].join(",");
    let sql = format!(
        "SELECT v.file_path, v.file_name, v.file_state, t.name, {eff}
         FROM videos v JOIN traps t ON t.id = v.trap_id
         WHERE v.sequence_id IN ({holes}) AND v.deleted_at IS NULL
         ORDER BY {eff}, v.file_name",
        eff = EFFECTIVE_RECORDED_AT
    );
    let params: Vec<&dyn ToSql> = sequence_ids
        .iter()
        .map(|s| s as &dyn ToSql)
        .collect();

    let mut report = CopyReport {
        dest: dest_dir.display().to_string(),
        ..Default::default()
    };

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params.as_slice())?;
    while let Some(r) = rows.next()? {
        let file_path: String = r.get(0)?;
        let file_name: String = r.get(1)?;
        let file_state: String = r.get(2)?;
        let trap_name: String = r.get(3)?;
        let recorded_at: Option<String> = r.get(4)?;

        if file_state != "present" {
            report.unavailable += 1;
            continue;
        }
        let source = PathBuf::from(&file_path);
        if !source.exists() {
            report.unavailable += 1;
            continue;
        }

        // Les caméras nomment toutes leurs fichiers IMG_0001 : sans préfixe, copier
        // deux pièges dans un même dossier écraserait la moitié des vidéos.
        let stamp = recorded_at
            .as_deref()
            .and_then(|d| d.get(..16).map(|s| s.replace(['-', ':', 'T'], "").replace(' ', "-")))
            .unwrap_or_else(|| "sans-date".into());
        let target = unique_path(
            dest_dir,
            &format!("{}_{}_{}", safe(&trap_name), stamp, safe(&file_name)),
        );

        match std::fs::copy(&source, &target) {
            Ok(bytes) => {
                report.copied += 1;
                report.bytes += bytes;
            }
            Err(e) => report.errors.push(format!("{file_name} : {e}")),
        }
    }

    Ok(report)
}

/// Un nom libre dans le dossier : on n'écrase jamais un fichier existant, même si
/// c'est une copie antérieure de la même vidéo.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 2..10_000 {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem} ({}){ext}", uuid::Uuid::new_v4()))
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
             VALUES ('t1', 'Mare basse', 47.32, 5.04, datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sequences (id, trap_id, started_at, ended_at, video_count,
                 auto_grouped, created_at, updated_at)
             VALUES ('s1', 't1', '2026-03-14 21:00:00', '2026-03-14 21:05:00', 2, 1,
                     datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn
    }

    fn species_id(conn: &Connection) -> String {
        conn.query_row("SELECT id FROM species LIMIT 1", [], |r| r.get(0))
            .unwrap()
    }

    fn read(path: &Path) -> String {
        String::from_utf8_lossy(&std::fs::read(path).unwrap()).to_string()
    }

    #[test]
    fn echappe_les_champs_qui_contiennent_le_separateur() {
        assert_eq!(field("simple"), "simple");
        assert_eq!(field("avec;point-virgule"), "\"avec;point-virgule\"");
        assert_eq!(field("avec\"guillemet"), "\"avec\"\"guillemet\"");
        assert_eq!(field("deux\nlignes"), "\"deux\nlignes\"");
    }

    #[test]
    fn le_csv_des_sequences_porte_le_bom_et_une_ligne_par_passage() {
        let conn = db();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("sequences.csv");

        let r = sequences_csv(&conn, GridFilter::default(), &dest).unwrap();
        assert_eq!(r.rows, 1);

        let bytes = std::fs::read(&dest).unwrap();
        assert_eq!(&bytes[..3], BOM, "sans BOM, Excel massacre les accents");

        let text = read(&dest);
        assert!(text.contains("id_sequence;piege"));
        assert!(text.contains("Mare basse"));
        assert!(text.contains("47.320000"));
    }

    #[test]
    fn une_note_avec_un_point_virgule_ne_decale_pas_les_colonnes() {
        let mut conn = db();
        annotate(
            &mut conn,
            &["s1".to_string()],
            Annotation {
                notes: Some("laie suitée ; quatre marcassins".into()),
                ..Default::default()
            },
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("s.csv");
        sequences_csv(&conn, GridFilter::default(), &dest).unwrap();

        let text = read(&dest);
        let data_line = text.lines().nth(1).unwrap();
        assert!(text.contains("\"laie suitée ; quatre marcassins\""));
        // Le nombre de colonnes doit rester celui de l'en-tête.
        let count_outside_quotes = |s: &str| {
            let mut inside = false;
            s.chars()
                .filter(|c| {
                    if *c == '"' {
                        inside = !inside;
                    }
                    *c == ';' && !inside
                })
                .count()
        };
        let header = text.lines().next().unwrap();
        assert_eq!(
            count_outside_quotes(data_line),
            count_outside_quotes(header),
            "un point-virgule dans une note ne doit pas créer de colonne"
        );
    }

    #[test]
    fn le_csv_des_detections_a_une_ligne_par_espece() {
        let mut conn = db();
        let mut stmt = conn.prepare("SELECT id FROM species LIMIT 2").unwrap();
        let sp: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        drop(stmt);

        annotate(
            &mut conn,
            &["s1".to_string()],
            Annotation {
                add_species: vec![
                    SpeciesPick {
                        species_id: sp[0].clone(),
                        confidence: "certain".into(),
                        count_min: Some(1),
                        count_max: Some(3),
                    },
                    SpeciesPick {
                        species_id: sp[1].clone(),
                        confidence: "possible".into(),
                        count_min: None,
                        count_max: None,
                    },
                ],
                ..Default::default()
            },
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("detections.csv");
        let r = detections_csv(&conn, GridFilter::default(), &dest).unwrap();
        assert_eq!(r.rows, 2, "une ligne par espèce, pas une par séquence");

        let text = read(&dest);
        assert!(text.contains(";2026;3;21;"), "année, mois et heure sont dépliés");
        assert!(text.contains("certain"));
        assert!(text.contains("possible"));
    }

    #[test]
    fn une_sequence_sans_espece_est_absente_des_detections() {
        let conn = db();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("d.csv");
        let r = detections_csv(&conn, GridFilter::default(), &dest).unwrap();
        assert_eq!(r.rows, 0);
        // L'en-tête est là malgré tout : un fichier vide ne dit pas s'il a échoué.
        assert!(read(&dest).contains("id_sequence"));
    }

    #[test]
    fn les_filtres_s_appliquent_a_l_export() {
        let conn = db();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("s.csv");
        let r = sequences_csv(
            &conn,
            GridFilter {
                months: vec![6],
                ..Default::default()
            },
            &dest,
        )
        .unwrap();
        assert_eq!(r.rows, 0, "la séquence de mars n'est pas exportée");
    }

    #[test]
    fn nettoie_les_noms_de_fichier() {
        assert_eq!(safe("Mare basse"), "Mare basse");
        assert_eq!(safe("Coulée / ruisseau"), "Coulée - ruisseau");
        assert_eq!(safe("a:b*c?d"), "a-b-c-d");
        assert!(safe(&"x".repeat(200)).len() <= 60);
    }

    #[test]
    fn copie_sans_deplacer_et_sans_ecraser() {
        let conn = db();
        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();

        let source = src_dir.path().join("IMG_0001.mp4");
        std::fs::write(&source, b"des octets de video").unwrap();
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id, sequence_id,
                 recorded_at, file_state, imported_at, created_at, updated_at)
             VALUES ('v1', ?1, 'IMG_0001.mp4', 'h1', 't1', 's1', '2026-03-14T21:00:00Z',
                     'present', datetime('now'), datetime('now'), datetime('now'))",
            params![source.display().to_string()],
        )
        .unwrap();

        let r = copy_videos(&conn, &["s1".to_string()], dest_dir.path()).unwrap();
        assert_eq!(r.copied, 1);
        assert!(source.exists(), "l'original reste en place : on copie, on ne déplace pas");

        let copies: Vec<_> = std::fs::read_dir(dest_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(copies.len(), 1);
        assert!(
            copies[0].starts_with("Mare basse_202603"),
            "le nom porte le piège et la date, got {copies:?}"
        );

        // Rejouer la copie ne doit rien écraser.
        let r = copy_videos(&conn, &["s1".to_string()], dest_dir.path()).unwrap();
        assert_eq!(r.copied, 1);
        assert_eq!(
            std::fs::read_dir(dest_dir.path()).unwrap().count(),
            2,
            "la seconde copie prend un nom libre"
        );
    }

    #[test]
    fn une_video_supprimee_est_comptee_pas_silencieuse() {
        let conn = db();
        let dest_dir = tempfile::tempdir().unwrap();
        conn.execute(
            "INSERT INTO videos (id, file_path, file_name, content_hash, trap_id, sequence_id,
                 file_state, imported_at, created_at, updated_at)
             VALUES ('v1', '/nulle/part.mp4', 'part.mp4', 'h1', 't1', 's1',
                     'purged', datetime('now'), datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        let r = copy_videos(&conn, &["s1".to_string()], dest_dir.path()).unwrap();
        assert_eq!(r.copied, 0);
        assert_eq!(r.unavailable, 1, "la donnée reste, le fichier non");
    }

    #[test]
    fn refuse_un_dossier_de_destination_inexistant() {
        let conn = db();
        assert!(copy_videos(&conn, &["s1".to_string()], Path::new("/nulle/part")).is_err());
    }
}

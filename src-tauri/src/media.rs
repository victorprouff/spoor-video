//! Appels à ffprobe et ffmpeg : date de capture, caractéristiques, vignettes.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use chrono::{DateTime, NaiveDateTime, Utc};

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("ffmpeg est introuvable ; l'indexation ne peut pas lire les vidéos")]
    NotFound,
    #[error("{tool} a échoué : {message}")]
    Failed { tool: String, message: String },
    #[error("erreur d'entrée-sortie : {0}")]
    Io(#[from] std::io::Error),
}

/// Emplacements sondés, dans l'ordre.
///
/// Une application lancée depuis le Finder n'hérite **pas** du `PATH` du shell :
/// `/opt/homebrew/bin` y est absent, et se contenter de `PATH` donnerait un outil
/// qui marche en développement et pas une fois installé. On sonde donc explicitement
/// les emplacements usuels.
fn candidates(tool: &str, resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // 1. binaire embarqué dans l'application (cible : §4 de la spec)
    if let Some(dir) = resource_dir {
        out.push(dir.join("bin").join(tool));
    }
    // 2. emplacements courants sur macOS
    out.push(PathBuf::from("/opt/homebrew/bin").join(tool));
    out.push(PathBuf::from("/usr/local/bin").join(tool));
    out.push(PathBuf::from("/usr/bin").join(tool));
    // 3. PATH, en dernier recours
    out.push(PathBuf::from(tool));
    out
}

fn resolve(tool: &str, resource_dir: Option<&Path>) -> Option<PathBuf> {
    for path in candidates(tool, resource_dir) {
        let ok = Command::new(&path)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return Some(path);
        }
    }
    None
}

/// Résolu une fois par exécution : sonder cinq chemins pour chacune des milliers de
/// vidéos d'une passe serait absurde.
static TOOLS: OnceLock<(Option<PathBuf>, Option<PathBuf>)> = OnceLock::new();

pub fn init(resource_dir: Option<&Path>) {
    let _ = TOOLS.get_or_init(|| {
        let probe = resolve("ffprobe", resource_dir);
        let ffmpeg = resolve("ffmpeg", resource_dir);
        match (&probe, &ffmpeg) {
            (Some(p), Some(f)) => log::info!("ffprobe: {} — ffmpeg: {}", p.display(), f.display()),
            _ => log::warn!("ffmpeg/ffprobe introuvables"),
        }
        (probe, ffmpeg)
    });
}

fn ffprobe_path() -> Result<PathBuf, MediaError> {
    TOOLS
        .get()
        .and_then(|(p, _)| p.clone())
        .ok_or(MediaError::NotFound)
}

fn ffmpeg_path() -> Result<PathBuf, MediaError> {
    TOOLS
        .get()
        .and_then(|(_, f)| f.clone())
        .ok_or(MediaError::NotFound)
}

pub fn available() -> bool {
    TOOLS
        .get()
        .map(|(p, f)| p.is_some() && f.is_some())
        .unwrap_or(false)
}

/// Ce que ffprobe sait dire d'une vidéo.
#[derive(Debug, Default, Clone)]
pub struct Probe {
    /// Date de capture. `None` quand le fichier n'en porte pas : elle n'est **jamais**
    /// remplacée par la date du jour ni par la date de modification du fichier, qui
    /// change au moindre copier-coller (§4).
    pub recorded_at: Option<DateTime<Utc>>,
    pub duration_s: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub fps: Option<f64>,
}

pub fn probe(path: &Path) -> Result<Probe, MediaError> {
    let out = Command::new(ffprobe_path()?)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()?;

    if !out.status.success() {
        return Err(MediaError::Failed {
            tool: "ffprobe".into(),
            message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| {
        MediaError::Failed {
            tool: "ffprobe".into(),
            message: format!("sortie illisible : {e}"),
        }
    })?;

    let tags = json.pointer("/format/tags");
    let recorded_at = tags
        .and_then(|t| {
            // Les pièges photo n'écrivent pas tous la même étiquette.
            ["creation_time", "date", "com.apple.quicktime.creationdate"]
                .iter()
                .find_map(|k| t.get(*k).and_then(|v| v.as_str()))
        })
        .and_then(parse_timestamp);

    let video_stream = json
        .get("streams")
        .and_then(|s| s.as_array())
        .and_then(|streams| {
            streams
                .iter()
                .find(|s| s.get("codec_type").and_then(|c| c.as_str()) == Some("video"))
        });

    Ok(Probe {
        recorded_at,
        duration_s: json
            .pointer("/format/duration")
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse().ok()),
        width: video_stream
            .and_then(|s| s.get("width"))
            .and_then(|v| v.as_i64()),
        height: video_stream
            .and_then(|s| s.get("height"))
            .and_then(|v| v.as_i64()),
        fps: video_stream
            .and_then(|s| s.get("avg_frame_rate"))
            .and_then(|v| v.as_str())
            .and_then(parse_fraction),
    })
}

/// Les formats de date rencontrés dans les conteneurs vidéo.
///
/// Une date **sans fuseau** est interprétée en UTC faute de mieux, mais c'est une
/// convention, pas une certitude : les pièges photo écrivent souvent l'heure locale
/// de leur horloge interne. La correction manuelle (`recorded_at_manual`) est là pour ça.
pub fn parse_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y:%m:%d %H:%M:%S",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, fmt) {
            return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
        }
    }
    None
}

fn parse_fraction(raw: &str) -> Option<f64> {
    let (n, d) = raw.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    if d == 0.0 {
        return None;
    }
    Some(n / d)
}

/// Extrait une vignette JPEG. `at_s` est la position dans la vidéo.
///
/// Sur un piège photo, la toute première image est souvent noire (l'infrarouge n'a pas
/// encore éclairé) : on prend une image un peu plus loin.
pub fn thumbnail(src: &Path, dest: &Path, at_s: f64, width: u32) -> Result<(), MediaError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = Command::new(ffmpeg_path()?)
        .args(["-v", "quiet", "-y", "-ss", &format!("{at_s:.2}")])
        .arg("-i")
        .arg(src)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={width}:-2"),
            "-q:v",
            "4",
        ])
        .arg(dest)
        .output()?;

    if !out.status.success() || !dest.exists() {
        return Err(MediaError::Failed {
            tool: "ffmpeg".into(),
            message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_les_formats_de_date_rencontres() {
        assert!(parse_timestamp("2026-03-14T22:31:05.000000Z").is_some());
        assert!(parse_timestamp("2026-03-14 22:31:05").is_some());
        assert!(parse_timestamp("2026:03:14 22:31:05").is_some());
        assert!(parse_timestamp("").is_none());
        assert!(parse_timestamp("pas une date").is_none());
    }

    #[test]
    fn lit_les_frequences_en_fraction() {
        assert_eq!(parse_fraction("30/1"), Some(30.0));
        assert_eq!(parse_fraction("30000/1001").map(|f| f.round()), Some(30.0));
        // Une image fixe donne 0/0 : ne pas diviser par zéro.
        assert_eq!(parse_fraction("0/0"), None);
        assert_eq!(parse_fraction("30"), None);
    }
}

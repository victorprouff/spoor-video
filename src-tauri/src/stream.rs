//! Serveur HTTP local qui sert les vidéos à la fenêtre, **sous Linux seulement**.
//!
//! Sous Linux, la balise `<video>` de WebKitGTK confie le chargement à GStreamer, qui ne
//! connaît que `http(s)`, `blob` et `file` : le schéma `asset://` de Tauri y échoue
//! aussitôt (`FormatError`), quel que soit le codec. Les images, elles, passent par le
//! chargeur de WebKit et s'affichent. Sur Mac, `asset://` lit très bien les vidéos et ce
//! serveur n'est pas démarré.
//!
//! Garde-fous : écoute sur `127.0.0.1` uniquement, jeton aléatoire tiré à chaque
//! lancement dans chaque adresse, et seuls les fichiers que le protocole `asset`
//! autorise (la racine des vidéos) sont servis — même règle, même contrôle.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

/// Adresse de base à laquelle le front ajoute `?path=…`.
pub struct VideoServer {
    pub base_url: String,
}

/// Démarre le serveur sur un port libre. `allowed` décide si un chemin peut être servi.
pub fn start<F>(allowed: F) -> io::Result<VideoServer>
where
    F: Fn(&Path) -> bool + Send + Sync + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let token = uuid::Uuid::new_v4().simple().to_string();
    let base_url = format!("http://127.0.0.1:{port}/{token}");
    log::info!("vidéos servies sur 127.0.0.1:{port}");
    let allowed = Arc::new(allowed);

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let allowed = Arc::clone(&allowed);
            let token = token.clone();
            // Une lecture fait plusieurs requêtes partielles concurrentes : un fil par
            // connexion, la fenêtre n'en ouvre jamais beaucoup.
            thread::spawn(move || {
                if let Err(e) = handle(stream, &token, allowed.as_ref()) {
                    // Le plus souvent, la fenêtre a coupé la connexion (saut dans la vidéo).
                    log::debug!("vidéo : connexion interrompue : {e}");
                }
            });
        }
    });

    Ok(VideoServer { base_url })
}

fn handle(stream: TcpStream, token: &str, allowed: &dyn Fn(&Path) -> bool) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;

    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut range = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("range") {
                range = Some(value.trim().to_string());
            }
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    if method != "GET" && method != "HEAD" {
        return status(&mut out, "405 Method Not Allowed");
    }
    let Some(path) = requested_path(target, token) else {
        return status(&mut out, "404 Not Found");
    };
    if !allowed(&path) {
        log::warn!("vidéo refusée, hors du dossier autorisé : {}", path.display());
        return status(&mut out, "403 Forbidden");
    }
    let Ok(mut file) = File::open(&path) else {
        return status(&mut out, "404 Not Found");
    };
    let size = file.metadata()?.len();

    let (start, end, code) = match range.as_deref().map(|r| parse_range(r, size)) {
        None => (0, size.saturating_sub(1), "200 OK"),
        Some(Some((s, e))) => (s, e, "206 Partial Content"),
        Some(None) => {
            write!(
                out,
                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{size}\r\n\
                 Content-Length: 0\r\nConnection: close\r\n\r\n"
            )?;
            return Ok(());
        }
    };
    let len = if size == 0 { 0 } else { end - start + 1 };

    let mut head = format!(
        "HTTP/1.1 {code}\r\nContent-Type: {}\r\nContent-Length: {len}\r\n\
         Accept-Ranges: bytes\r\nCache-Control: no-store\r\nConnection: close\r\n",
        content_type(&path)
    );
    if code.starts_with("206") {
        head.push_str(&format!("Content-Range: bytes {start}-{end}/{size}\r\n"));
    }
    head.push_str("\r\n");
    out.write_all(head.as_bytes())?;

    if method == "GET" && len > 0 {
        file.seek(SeekFrom::Start(start))?;
        io::copy(&mut file.take(len), &mut out)?;
    }
    out.flush()
}

fn status(out: &mut TcpStream, code: &str) -> io::Result<()> {
    write!(out, "HTTP/1.1 {code}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
}

/// `/{jeton}?path=<chemin encodé>` → chemin. Tout autre cible est refusée.
fn requested_path(target: &str, token: &str) -> Option<PathBuf> {
    let (route, query) = target.split_once('?')?;
    if route.strip_prefix('/')? != token {
        return None;
    }
    let encoded = query.split('&').find_map(|kv| kv.strip_prefix("path="))?;
    let path = PathBuf::from(percent_decode(encoded)?);
    path.is_absolute().then_some(path)
}

/// Décode `%XX` (et `+` laissé tel quel : `encodeURIComponent` encode l'espace en `%20`).
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `bytes=a-b`, `bytes=a-` ou `bytes=-n` → bornes incluses, ou `None` si insatisfiable.
fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    if size == 0 {
        return None;
    }
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        if n == 0 {
            return None;
        }
        (size.saturating_sub(n), size - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() { size - 1 } else { b.parse::<u64>().ok()?.min(size - 1) };
        (start, end)
    };
    (start <= end && start < size).then_some((start, end))
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("avi") => "video/x-msvideo",
        Some("mkv") => "video/x-matroska",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(base: &str, path: &str, range: Option<&str>) -> (String, Vec<u8>) {
        let rest = base.strip_prefix("http://").unwrap();
        let (host, route) = rest.split_once('/').unwrap();
        let mut s = TcpStream::connect(host).unwrap();
        let encoded: String = path
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"/-_.".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        let mut req = format!("GET /{route}?path={encoded} HTTP/1.1\r\nHost: {host}\r\n");
        if let Some(r) = range {
            req.push_str(&format!("Range: {r}\r\n"));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes()).unwrap();
        let mut resp = Vec::new();
        s.read_to_end(&mut resp).unwrap();
        let split = resp.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (String::from_utf8_lossy(&resp[..split]).into_owned(), resp[split + 4..].to_vec())
    }

    #[test]
    fn plage_complete_ouverte_et_suffixe() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=500-", 1000), Some((500, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=900-5000", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("bytes=50-10", 1000), None);
    }

    #[test]
    fn chemin_avec_accents_et_espaces_decode() {
        let p = requested_path("/abc?path=%2Fh%2FPi%C3%A8ge%20photo%2Fa.MP4", "abc");
        assert_eq!(p, Some(PathBuf::from("/h/Piège photo/a.MP4")));
    }

    #[test]
    fn mauvais_jeton_ou_chemin_relatif_refuses() {
        assert_eq!(requested_path("/autre?path=%2Fa.mp4", "abc"), None);
        assert_eq!(requested_path("/abc?path=a.mp4", "abc"), None);
        assert_eq!(requested_path("/abc", "abc"), None);
    }

    #[test]
    fn sert_le_fichier_entier_puis_une_plage() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Piège vidéo.mp4");
        let data: Vec<u8> = (0..=255u8).cycle().take(10_000).collect();
        std::fs::write(&file, &data).unwrap();
        let root = dir.path().to_path_buf();
        let server = start(move |p| p.starts_with(&root)).unwrap();
        let path = file.to_str().unwrap();

        let (head, body) = get(&server.base_url, path, None);
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert!(head.contains("Content-Type: video/mp4"));
        assert_eq!(body, data);

        let (head, body) = get(&server.base_url, path, Some("bytes=100-199"));
        assert!(head.starts_with("HTTP/1.1 206"), "{head}");
        assert!(head.contains("Content-Range: bytes 100-199/10000"));
        assert_eq!(body, &data[100..200]);
    }

    #[test]
    fn refuse_un_fichier_hors_du_dossier_autorise() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("secret.txt");
        std::fs::write(&file, b"non").unwrap();
        let server = start(|_| false).unwrap();
        let (head, body) = get(&server.base_url, file.to_str().unwrap(), None);
        assert!(head.starts_with("HTTP/1.1 403"), "{head}");
        assert!(body.is_empty());
    }
}

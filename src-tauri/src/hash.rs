//! Empreinte de fichier.
//!
//! Elle porte trois besoins de la spec (§3, §4, §6) : ne jamais réimporter deux fois le
//! même fichier, **retrouver une vidéo déplacée ou renommée**, et ré-ignorer ce qui a été
//! supprimé sans trace quand la même carte SD repasse.
//!
//! On ne lit pas le fichier entier : une passe sur des milliers de vidéos de 30 Mo posées
//! sur un disque externe coûterait des dizaines de minutes pour un gain nul. On prend la
//! taille, le premier et le dernier mégaoctet. Deux vidéos distinctes d'un piège photo
//! diffèrent dès les premiers octets (horodatage dans le conteneur, contenu de l'image).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const CHUNK: u64 = 1024 * 1024;

pub fn content_hash(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();

    let mut hasher = blake3::Hasher::new();
    // La taille entre dans l'empreinte : deux fichiers partageant début et fin mais de
    // longueurs différentes sont bien distincts (vidéo tronquée par une carte pleine).
    hasher.update(&size.to_le_bytes());

    let mut buf = vec![0u8; CHUNK as usize];

    let head = CHUNK.min(size) as usize;
    file.read_exact(&mut buf[..head])?;
    hasher.update(&buf[..head]);

    // Queue lue seulement si elle ne recouvre pas la tête.
    if size > CHUNK {
        let tail_start = size.saturating_sub(CHUNK);
        let tail = (size - tail_start) as usize;
        file.seek(SeekFrom::Start(tail_start))?;
        file.read_exact(&mut buf[..tail])?;
        hasher.update(&buf[..tail]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("spoor-hash-{name}"));
        let mut f = File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        path
    }

    #[test]
    fn meme_contenu_meme_empreinte() {
        let a = tmp("a", b"des octets de video");
        let b = tmp("b", b"des octets de video");
        assert_eq!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
    }

    #[test]
    fn contenu_different_empreinte_differente() {
        let a = tmp("c", b"video une");
        let b = tmp("d", b"video deux");
        assert_ne!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
    }

    #[test]
    fn la_taille_distingue_les_fichiers_tronques() {
        // Même début, même fin, longueurs différentes : sans la taille dans l'empreinte,
        // une vidéo tronquée passerait pour la vidéo complète.
        let mut complet = vec![b'x'; (CHUNK * 3) as usize];
        complet[0] = b'A';
        *complet.last_mut().unwrap() = b'Z';
        let mut tronque = vec![b'x'; (CHUNK * 2) as usize];
        tronque[0] = b'A';
        *tronque.last_mut().unwrap() = b'Z';

        let a = tmp("e", &complet);
        let b = tmp("f", &tronque);
        assert_ne!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
    }

    #[test]
    fn fichier_vide_ne_panique_pas() {
        let a = tmp("g", b"");
        assert!(content_hash(&a).is_ok());
    }
}

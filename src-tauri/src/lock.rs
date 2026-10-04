//! Le verrou posé à côté de la base pendant qu'elle est ouverte.
//!
//! La base peut vivre dans un dossier synchronisé (kDrive) et servir à deux machines.
//! Ouverte sur les deux à la fois, kDrive garderait deux copies en conflit et le travail
//! de l'une serait perdu. Le verrou dit où la base est ouverte ; l'autre machine refuse
//! de l'ouvrir et le dit, sauf si on lui demande de passer outre (machine éteinte,
//! application plantée). Ce n'est pas une garantie : kDrive met quelques secondes à
//! faire voyager le verrou. C'est un garde-fou contre l'oubli.

use std::path::{Path, PathBuf};

use crate::db::DbError;

const LOCK_FILE: &str = "spoor-video.lock";

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LockInfo {
    /// Nom de la machine qui a ouvert la base.
    pub host: String,
    /// Heure locale de cette machine, sans fuseau : `2026-10-04T14:32:00`.
    pub since: String,
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(LOCK_FILE)
}

/// Le nom de cette machine, tel qu'on le montre à l'autre.
pub fn this_host() -> String {
    // Sur Mac, le nom choisi dans les Réglages (« MacBook de Victor ») plutôt que le nom
    // réseau (« MacBook-de-Victor.local »).
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("scutil").args(["--get", "ComputerName"]).output() {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && !name.is_empty() {
            return name;
        }
    }
    for file in ["/proc/sys/kernel/hostname", "/etc/hostname"] {
        if let Ok(name) = std::fs::read_to_string(file) {
            let name = name.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    std::env::var("HOSTNAME").unwrap_or_else(|_| "machine inconnue".into())
}

/// Le verrou présent dans `dir`, s'il y en a un. Un verrou illisible (copie en conflit
/// à moitié synchronisée) est traité comme posé par une machine inconnue : mieux vaut
/// demander que d'ouvrir à l'aveugle.
pub fn read(dir: &Path) -> Option<LockInfo> {
    let raw = std::fs::read_to_string(path(dir)).ok()?;
    Some(serde_json::from_str(&raw).unwrap_or(LockInfo {
        host: "machine inconnue".into(),
        since: String::new(),
    }))
}

/// Le verrou posé par une **autre** machine, s'il y en a un. Celui de cette machine
/// vient d'une ouverture qui ne s'est pas refermée proprement : on le reprend.
pub fn held_elsewhere(dir: &Path, host: &str) -> Option<LockInfo> {
    read(dir).filter(|l| l.host != host)
}

pub fn acquire(dir: &Path, host: &str) -> Result<(), DbError> {
    let info = LockInfo {
        host: host.to_string(),
        since: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };
    let json = serde_json::to_string_pretty(&info).map_err(|e| DbError::Other(e.to_string()))?;
    std::fs::write(path(dir), json)?;
    Ok(())
}

/// Retire le verrou, s'il est bien celui de cette machine : on ne lève jamais celui
/// d'une autre en partant.
pub fn release(dir: &Path, host: &str) {
    if read(dir).is_some_and(|l| l.host == host) {
        if let Err(e) = std::fs::remove_file(path(dir)) {
            log::warn!("verrou non retiré : {e}");
        }
    }
}

/// Passer outre : retire le verrou quel qu'il soit.
pub fn force(dir: &Path) -> Result<(), DbError> {
    let p = path(dir);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_verrou_d_une_autre_machine_est_signale() {
        let dir = tempfile::tempdir().unwrap();
        acquire(dir.path(), "MacBook").unwrap();
        let other = held_elsewhere(dir.path(), "pop-os").unwrap();
        assert_eq!(other.host, "MacBook");
        assert!(!other.since.is_empty());
    }

    #[test]
    fn le_verrou_de_cette_machine_est_repris() {
        let dir = tempfile::tempdir().unwrap();
        acquire(dir.path(), "pop-os").unwrap();
        assert!(held_elsewhere(dir.path(), "pop-os").is_none());
    }

    #[test]
    fn on_ne_leve_jamais_le_verrou_d_une_autre_machine_en_partant() {
        let dir = tempfile::tempdir().unwrap();
        acquire(dir.path(), "MacBook").unwrap();
        release(dir.path(), "pop-os");
        assert!(path(dir.path()).exists());
        release(dir.path(), "MacBook");
        assert!(!path(dir.path()).exists());
    }

    #[test]
    fn un_verrou_illisible_vaut_un_verrou_inconnu() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(path(dir.path()), "{ à moitié").unwrap();
        assert_eq!(held_elsewhere(dir.path(), "pop-os").unwrap().host, "machine inconnue");
    }

    #[test]
    fn passer_outre_retire_le_verrou() {
        let dir = tempfile::tempdir().unwrap();
        acquire(dir.path(), "MacBook").unwrap();
        force(dir.path()).unwrap();
        assert!(read(dir.path()).is_none());
    }
}

//! Protection des mots de passe enregistrés.
//!
//! Sous Windows : DPAPI (`CryptProtectData`), lié au compte Windows de l'utilisateur ; le
//! fichier de sites copié sur une autre machine ou un autre compte ne révèle rien.
//! Ailleurs : simple base64 (marqué comme tel), en attendant un trousseau système.
//!
//! Format stocké : `dpapi:<base64>` ou `b64:<base64>` ; chaîne vide = pas de mot de passe.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// Chiffre un secret pour l'enregistrer.
pub fn protect(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    #[cfg(windows)]
    if let Some(blob) = dpapi::protect(plain.as_bytes()) {
        return format!("dpapi:{}", STANDARD.encode(blob));
    }
    format!("b64:{}", STANDARD.encode(plain.as_bytes()))
}

/// Déchiffre un secret enregistré. `None` si illisible (autre compte Windows, fichier abîmé…).
pub fn unprotect(stored: &str) -> Option<String> {
    if stored.is_empty() {
        return Some(String::new());
    }
    let (kind, data) = stored.split_once(':')?;
    let bytes = STANDARD.decode(data).ok()?;
    let plain = match kind {
        #[cfg(windows)]
        "dpapi" => dpapi::unprotect(&bytes)?,
        "b64" => bytes,
        _ => return None,
    };
    String::from_utf8(plain).ok()
}

#[cfg(windows)]
mod dpapi {
    /// Application-specific entropy: another program using DPAPI cannot decrypt our
    /// secrets without knowing it.
    const ENTROPY: &[u8] = b"UltraRushFTP/sites/v1";

    pub fn protect(plain: &[u8]) -> Option<Vec<u8>> {
        urftp_sys::dpapi::protect(plain, ENTROPY).ok()
    }

    pub fn unprotect(blob: &[u8]) -> Option<Vec<u8>> {
        urftp_sys::dpapi::unprotect(blob, ENTROPY).ok()
    }
}

/// Sérialisation serde : le champ est chiffré dans le fichier, en clair en mémoire.
pub mod serde_secret {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &str, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::protect(value))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
        let stored = String::deserialize(d)?;
        // Un secret illisible ne doit pas empêcher d'ouvrir tout le carnet de sites.
        Ok(super::unprotect(&stored).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for p in ["", "s3cr3t", "mot de passe accentué é à ü 🔑"] {
            let stored = protect(p);
            assert!(!stored.contains(p) || p.is_empty(), "le secret apparaît en clair : {stored}");
            assert_eq!(unprotect(&stored).as_deref(), Some(p));
        }
        #[cfg(windows)]
        assert!(protect("x").starts_with("dpapi:"));
        assert_eq!(unprotect("b64:dGVzdA==").as_deref(), Some("test"));
        assert_eq!(unprotect("inconnu:xx"), None);
    }
}

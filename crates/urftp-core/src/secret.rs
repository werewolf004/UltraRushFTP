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
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    /// Entropie propre à l'application : un autre logiciel utilisant DPAPI ne peut pas
    /// déchiffrer nos secrets sans la connaître.
    const ENTROPY: &[u8] = b"UltraRushFTP/sites/v1";

    fn call(input: &[u8], protect: bool) -> Option<Vec<u8>> {
        let data_in = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_ptr() as *mut u8 };
        let entropy = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY : les blobs d'entrée pointent sur des tampons valides pendant l'appel ; le
        // tampon de sortie est alloué par Windows et libéré par LocalFree après copie.
        let ok = unsafe {
            if protect {
                CryptProtectData(&data_in, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            } else {
                CryptUnprotectData(&data_in, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            }
        };
        if ok == 0 || out.pbData.is_null() {
            return None;
        }
        // SAFETY : `out` décrit un tampon de `cbData` octets alloué par CryptProtectData.
        let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
        unsafe { LocalFree(out.pbData.cast()) };
        Some(bytes)
    }

    pub fn protect(plain: &[u8]) -> Option<Vec<u8>> {
        call(plain, true)
    }

    pub fn unprotect(blob: &[u8]) -> Option<Vec<u8>> {
        call(blob, false)
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

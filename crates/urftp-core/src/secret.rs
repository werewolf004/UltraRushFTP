//! Protection of stored passwords.
//!
//! On Windows: DPAPI (`CryptProtectData`), bound to the user's Windows account; a site
//! file copied to another machine or account reveals nothing.
//! Elsewhere: plain base64 (labelled as such), until a system keyring is used.
//!
//! Stored format: `dpapi:<base64>` or `b64:<base64>`; empty string = no password.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// Encrypts a secret before storing it.
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

/// Decrypts a stored secret. `None` when unreadable (other Windows account, damaged file...).
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

/// serde serialisation: the field is encrypted in the file, plain in memory.
pub mod serde_secret {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &str, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::protect(value))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
        let stored = String::deserialize(d)?;
        // An unreadable secret must not prevent opening the whole site book.
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
            assert!(!stored.contains(p) || p.is_empty(), "the secret appears in clear: {stored}");
            assert_eq!(unprotect(&stored).as_deref(), Some(p));
        }
        #[cfg(windows)]
        assert!(protect("x").starts_with("dpapi:"));
        assert_eq!(unprotect("b64:dGVzdA==").as_deref(), Some("test"));
        assert_eq!(unprotect("inconnu:xx"), None);
    }
}

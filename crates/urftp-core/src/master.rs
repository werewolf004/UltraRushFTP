//! Master password of the interface: only a salted PBKDF2-SHA256 hash is stored. It keeps
//! the site manager, the options and a locked window closed to other people at the
//! computer; the site passwords themselves stay protected by Windows (DPAPI).

use std::num::NonZeroU32;

use ring::rand::SecureRandom;

const ITERATIONS: u32 = 210_000;
const PREFIX: &str = "pbkdf2-sha256";

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    (0..s.len()).step_by(2).map(|i| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect()
}

fn derive(password: &str, salt: &[u8], iterations: NonZeroU32) -> [u8; 32] {
    let mut out = [0u8; 32];
    ring::pbkdf2::derive(ring::pbkdf2::PBKDF2_HMAC_SHA256, iterations, salt, password.as_bytes(), &mut out);
    out
}

/// Errors of [`hash_password`].
#[derive(Debug, thiserror::Error)]
pub enum MasterError {
    #[error("the system random generator failed")]
    Random,
}

/// `pbkdf2-sha256$<iterations>$<salt>$<hash>` of a password.
pub fn hash_password(password: &str) -> Result<String, MasterError> {
    let mut salt = [0u8; 16];
    ring::rand::SystemRandom::new().fill(&mut salt).map_err(|_| MasterError::Random)?;
    let iterations = NonZeroU32::new(ITERATIONS).ok_or(MasterError::Random)?;
    Ok(format!("{PREFIX}${ITERATIONS}${}${}", hex(&salt), hex(&derive(password, &salt, iterations))))
}

/// Does `password` match a stored hash? A malformed hash matches nothing.
pub fn verify_password(password: &str, stored: &str) -> bool {
    let mut parts = stored.split('$');
    let (Some(PREFIX), Some(iter), Some(salt), Some(hash), None) = (parts.next(), parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let (Some(iterations), Some(salt), Some(hash)) = (iter.parse().ok().and_then(NonZeroU32::new), unhex(salt), unhex(hash)) else {
        return false;
    };
    ring::pbkdf2::verify(ring::pbkdf2::PBKDF2_HMAC_SHA256, iterations, &salt, password.as_bytes(), &hash).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_salted_and_checked() {
        let a = hash_password("correct horse").unwrap();
        let b = hash_password("correct horse").unwrap();
        assert_ne!(a, b, "a new salt each time");
        assert!(a.starts_with("pbkdf2-sha256$210000$"));
        assert!(verify_password("correct horse", &a) && verify_password("correct horse", &b));
        assert!(!verify_password("Correct horse", &a));
        assert!(!verify_password("x", "") && !verify_password("x", "pbkdf2-sha256$0$00$00") && !verify_password("x", "md5$1$aa$bb"));
        assert!(!a.contains("correct"));
    }
}

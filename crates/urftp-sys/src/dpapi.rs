//! Windows DPAPI: secrets encrypted for the current Windows user account
//! (`CryptProtectData`). A blob copied to another machine or account cannot be decrypted.

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData};

/// DPAPI failure.
#[derive(Debug, thiserror::Error)]
pub enum DpapiError {
    /// Input larger than 4 GiB.
    #[error("DPAPI input too large ({0} bytes)")]
    TooLarge(usize),
    /// Windows refused the operation (other account, corrupted blob, wrong entropy...).
    #[error("DPAPI: {0}")]
    Os(#[source] std::io::Error),
}

fn blob(data: &[u8]) -> Result<CRYPT_INTEGER_BLOB, DpapiError> {
    let len = u32::try_from(data.len()).map_err(|_| DpapiError::TooLarge(data.len()))?;
    // DPAPI never writes through input blobs; the `*mut` is only an API artefact.
    Ok(CRYPT_INTEGER_BLOB { cbData: len, pbData: data.as_ptr().cast_mut() })
}

fn call(input: &[u8], entropy: &[u8], protect: bool) -> Result<Vec<u8>, DpapiError> {
    let data_in = blob(input)?;
    let entropy = blob(entropy)?;
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY: the input blobs point to buffers that stay alive across the synchronous
    // call; the optional pointers are null as allowed by the API; `out` receives a buffer
    // allocated by Windows, released below with `LocalFree`.
    let ok = unsafe {
        if protect {
            CryptProtectData(&data_in, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        } else {
            CryptUnprotectData(
                &data_in,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        }
    };
    if ok == 0 || out.pbData.is_null() {
        return Err(DpapiError::Os(std::io::Error::last_os_error()));
    }
    // SAFETY: on success `out` describes `cbData` initialised bytes owned by us.
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    // SAFETY: the buffer was allocated by DPAPI with `LocalAlloc` and is freed once.
    unsafe { LocalFree(out.pbData.cast()) };
    Ok(bytes)
}

/// Encrypts `plain` for the current user. `entropy` is an application secret that other
/// DPAPI users must also know to decrypt.
pub fn protect(plain: &[u8], entropy: &[u8]) -> Result<Vec<u8>, DpapiError> {
    call(plain, entropy, true)
}

/// Decrypts a blob produced by [`protect`] with the same `entropy`.
pub fn unprotect(blob: &[u8], entropy: &[u8]) -> Result<Vec<u8>, DpapiError> {
    call(blob, entropy, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_wrong_entropy() {
        let blob = protect(b"s3cr3t", b"app").unwrap();
        assert!(!blob.windows(6).any(|w| w == b"s3cr3t"));
        assert_eq!(unprotect(&blob, b"app").unwrap(), b"s3cr3t");
        assert!(matches!(unprotect(&blob, b"other"), Err(DpapiError::Os(_))));
        assert!(unprotect(b"garbage", b"app").is_err());
    }
}

//! Petites aides Win32 partagées par l'application et la DLL (chaînes, mémoire partagée).

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW, PAGE_READWRITE, UnmapViewOfFile,
};

/// Chaîne UTF-16 terminée par zéro, pour les API `…W`.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Texte reçu en octets (« ANSI » de mIRC / Delphi) : UTF-8 si valide, sinon Windows-1252.
pub fn decode_ansi(bytes: &[u8]) -> String {
    let bytes = bytes.split(|&b| b == 0).next().unwrap_or_default();
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        // Windows-1252 ≈ Latin-1 pour l'essentiel des caractères accentués.
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// Vue sur une mémoire partagée nommée (`FTPRush`, `mIRC`).
pub struct SharedMem {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    size: usize,
}

// SAFETY : la vue est une simple zone mémoire ; l'accès concurrent est réglé par le
// protocole (requête synchrone par SendMessage).
unsafe impl Send for SharedMem {}
// SAFETY : lectures et écritures sont de simples copies d'octets ; le protocole (un seul
// échange synchrone à la fois) évite les accès simultanés à une même zone.
unsafe impl Sync for SharedMem {}

impl SharedMem {
    /// Crée (ou ouvre si elle existe) la mémoire partagée `name`.
    pub fn create(name: &str, size: usize) -> Option<Self> {
        let n = wide(name);
        // SAFETY : paramètres valides ; le handle est fermé dans Drop.
        let handle = unsafe { CreateFileMappingW(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE, 0, size as u32, n.as_ptr()) };
        Self::map(handle, size)
    }

    /// Ouvre une mémoire partagée existante.
    pub fn open(name: &str, size: usize) -> Option<Self> {
        let n = wide(name);
        // SAFETY : idem.
        let handle = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, n.as_ptr()) };
        Self::map(handle, size)
    }

    fn map(handle: HANDLE, size: usize) -> Option<Self> {
        if handle.is_null() {
            return None;
        }
        // SAFETY : handle de mappage valide.
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size) };
        if view.Value.is_null() {
            unsafe { CloseHandle(handle) };
            return None;
        }
        Some(Self { handle, view, size })
    }

    fn ptr(&self) -> *mut u8 {
        self.view.Value.cast()
    }

    /// Lit la chaîne terminée par zéro au début de la zone.
    pub fn read_string(&self) -> String {
        // SAFETY : la vue fait `size` octets.
        let bytes = unsafe { std::slice::from_raw_parts(self.ptr(), self.size) };
        decode_ansi(bytes)
    }

    /// Écrit une chaîne (tronquée à la taille de la zone) suivie d'un zéro.
    pub fn write_string(&self, s: &str) {
        let bytes = s.as_bytes();
        let n = bytes.len().min(self.size - 1);
        // SAFETY : on écrit au plus `size` octets dans la vue.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.ptr(), n);
            *self.ptr().add(n) = 0;
        }
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.view.Value
    }
}

impl Drop for SharedMem {
    fn drop(&mut self) {
        // SAFETY : vue et handle obtenus à la création.
        unsafe {
            UnmapViewOfFile(self.view);
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_decoding_and_shared_memory() {
        assert_eq!(decode_ansi(b"abc\0xyz"), "abc");
        assert_eq!(decode_ansi("éà".as_bytes()), "éà");
        assert_eq!(decode_ansi(&[0x63, 0x61, 0x66, 0xE9]), "café", "repli Latin-1");
        let name = format!("urftp-test-{}", std::process::id());
        let a = SharedMem::create(&name, 64).unwrap();
        let b = SharedMem::open(&name, 64).unwrap();
        a.write_string("bonjour");
        assert_eq!(b.read_string(), "bonjour");
        b.write_string(&"x".repeat(200));
        assert_eq!(a.read_string().len(), 63, "tronqué à la taille de la zone");
    }
}

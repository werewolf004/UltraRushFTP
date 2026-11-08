//! System layer of UltraRushFTP: the **only** crate of the workspace allowed to contain
//! `unsafe` code. Every other crate is compiled with `#![forbid(unsafe_code)]`.
//!
//! It wraps, behind safe APIs:
//! - [`dpapi`]: Windows DPAPI (`CryptProtectData` / `CryptUnprotectData`) for stored passwords;
//! - [`win`]: UTF-16 strings and named shared memory;
//! - [`server`]: the hidden window the mIRC DLL talks to (application side);
//! - [`power`]: sleep, hibernate and shut down at the end of a queue;
//! - `mirc` (feature `mirc-dll`): the `rushmirc.dll` exports called by mIRC.
//!
//! Rules for this crate: every `unsafe` block carries a `// SAFETY:` comment
//! (`clippy::undocumented_unsafe_blocks` is denied), `unsafe fn` bodies need explicit
//! `unsafe` blocks, and no `unwrap` / `expect` outside tests.
//!
//! # rushmirc protocol
//!
//! The application creates a hidden top-level window (the DLL finds it with `FindWindow`)
//! of class [`OWN_WINDOW_CLASS`], or [`WINDOW_CLASS`] in FTPRush compatibility mode, and
//! handles:
//! - `WM_EXCHANGE`: an `Invoke` request read from / answered in the shared memory
//!   [`SHARED_MAP_NAME`];
//! - `WM_COPYDATA` with [`COPYDATA_SCRIPT`] or [`COPYDATA_COMMAND`];
//! - in return, `WM_COPYDATA` to the DLL callback window with `wParam` = [`CALLBACK_RAWCOMMAND`].

/// Class of the main window of FTPRush, looked up by its `rushmirc.dll`. FTPRush itself
/// looks for it when it starts and does not open when it finds one: UltraRushFTP only
/// uses it in compatibility mode ([`set_ftprush_compatible`]).
pub const WINDOW_CLASS: &str = "TfmRush";
/// Class of the UltraRushFTP window, looked up first by our `rushmirc.dll`.
pub const OWN_WINDOW_CLASS: &str = "TfmUltraRush";
/// Name of the application ↔ DLL shared memory.
pub const SHARED_MAP_NAME: &str = "FTPRush";
/// Name of the DLL → mIRC shared memory.
pub const MIRC_MAP_NAME: &str = "mIRC";
/// Size of both shared memories.
pub const SHARED_MAP_SIZE: usize = 4096;

/// `WM_USER`.
pub const WM_USER: u32 = 0x0400;
/// Message sent by `Invoke`.
pub const WM_EXCHANGE: u32 = WM_USER + 300;
/// Sent by a second UltraRushFTP to the running one (single instance): show the window.
pub const WM_SHOW_APP: u32 = WM_USER + 301;
/// mIRC message running the command stored in the `mIRC` shared memory.
pub const WM_MCOMMAND: u32 = WM_USER + 200;

/// `COPYDATASTRUCT.dwData` of a `RushScript` call.
pub const COPYDATA_SCRIPT: usize = 1000;
/// `COPYDATASTRUCT.dwData` of a `RushCommand` call.
pub const COPYDATA_COMMAND: usize = 1001;
/// `wParam` of the callback carrying the result of a raw command.
pub const CALLBACK_RAWCOMMAND: usize = 888;
/// Callback `dwData`: success.
pub const CALLBACK_SUCCESS: usize = 1;
/// Callback `dwData`: failure.
pub const CALLBACK_FAILED: usize = 0;

static FTPRUSH_COMPATIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Compatibility mode: the window takes the FTPRush class, so that the original
/// `rushmirc.dll` of FTPRush finds UltraRushFTP. FTPRush v2 then cannot start.
pub fn set_ftprush_compatible(on: bool) {
    FTPRUSH_COMPATIBLE.store(on, std::sync::atomic::Ordering::SeqCst);
}

/// Value of `URUSHFTP_IPC_CLASS` (tests: avoids talking to a real FTPRush or UltraRushFTP).
fn class_override() -> Option<String> {
    std::env::var("URUSHFTP_IPC_CLASS").ok().filter(|c| !c.is_empty())
}

/// Window class created by the application: `URUSHFTP_IPC_CLASS`, else [`WINDOW_CLASS`] in
/// compatibility mode, else [`OWN_WINDOW_CLASS`].
pub fn window_class() -> String {
    class_override().unwrap_or_else(|| {
        (if FTPRUSH_COMPATIBLE.load(std::sync::atomic::Ordering::SeqCst) { WINDOW_CLASS } else { OWN_WINDOW_CLASS }).to_string()
    })
}

/// Classes looked up by our DLL, in order: `URUSHFTP_IPC_CLASS` alone, else UltraRushFTP
/// then FTPRush.
pub fn dll_target_classes() -> Vec<String> {
    match class_override() {
        Some(c) => vec![c],
        None => vec![OWN_WINDOW_CLASS.to_string(), WINDOW_CLASS.to_string()],
    }
}

/// Text received as bytes ("ANSI" from mIRC / Delphi), cut at the first NUL: UTF-8 when
/// valid, Windows-1252 (≈ Latin-1) otherwise.
pub fn decode_ansi(bytes: &[u8]) -> String {
    let bytes = bytes.split(|&b| b == 0).next().unwrap_or_default();
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

#[cfg(windows)]
pub mod dpapi;
#[cfg(all(windows, feature = "mirc-dll"))]
pub mod mirc;
#[cfg(windows)]
pub mod power;
#[cfg(windows)]
pub mod server;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod win;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ftprush_class_is_only_taken_in_compatibility_mode() {
        if class_override().is_some() {
            return;
        }
        assert_eq!(window_class(), OWN_WINDOW_CLASS, "FTPRush v2 can start next to UltraRushFTP");
        set_ftprush_compatible(true);
        assert_eq!(window_class(), WINDOW_CLASS);
        set_ftprush_compatible(false);
        assert_eq!(dll_target_classes(), [OWN_WINDOW_CLASS, WINDOW_CLASS], "our DLL finds both applications");
    }

    #[test]
    fn ansi_decoding() {
        assert_eq!(decode_ansi(b"abc\0xyz"), "abc");
        assert_eq!(decode_ansi("éà".as_bytes()), "éà");
        assert_eq!(decode_ansi(&[0x63, 0x61, 0x66, 0xE9]), "café", "Latin-1 fallback");
        assert_eq!(decode_ansi(b""), "");
    }
}

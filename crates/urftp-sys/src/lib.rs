//! System layer of UltraRushFTP: the **only** crate of the workspace allowed to contain
//! `unsafe` code. Every other crate is compiled with `#![forbid(unsafe_code)]`.
//!
//! It wraps, behind safe APIs:
//! - [`dpapi`]: Windows DPAPI (`CryptProtectData` / `CryptUnprotectData`) for stored passwords;
//! - [`win`]: UTF-16 strings and named shared memory;
//! - [`server`]: the hidden `TfmRush` window the mIRC DLL talks to (application side);
//! - `mirc` (feature `mirc-dll`): the `rushmirc.dll` exports called by mIRC.
//!
//! Rules for this crate: every `unsafe` block carries a `// SAFETY:` comment
//! (`clippy::undocumented_unsafe_blocks` is denied), `unsafe fn` bodies need explicit
//! `unsafe` blocks, and no `unwrap` / `expect` outside tests.
//!
//! # rushmirc protocol
//!
//! The application creates a hidden top-level window of class [`WINDOW_CLASS`] (the DLL
//! finds it with `FindWindow`) and handles:
//! - `WM_EXCHANGE`: an `Invoke` request read from / answered in the shared memory
//!   [`SHARED_MAP_NAME`];
//! - `WM_COPYDATA` with [`COPYDATA_SCRIPT`] or [`COPYDATA_COMMAND`];
//! - in return, `WM_COPYDATA` to the DLL callback window with `wParam` = [`CALLBACK_RAWCOMMAND`].

/// Window class looked up by `rushmirc.dll`.
pub const WINDOW_CLASS: &str = "TfmRush";
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

/// Window class actually used: [`WINDOW_CLASS`], or the value of `URUSHFTP_IPC_CLASS`
/// (tests: avoids talking to a real FTPRush that is already running).
pub fn window_class() -> String {
    std::env::var("URUSHFTP_IPC_CLASS").ok().filter(|c| !c.is_empty()).unwrap_or_else(|| WINDOW_CLASS.to_string())
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
pub mod server;
#[cfg(windows)]
pub mod win;

#[cfg(test)]
mod tests {
    use super::decode_ansi;

    #[test]
    fn ansi_decoding() {
        assert_eq!(decode_ansi(b"abc\0xyz"), "abc");
        assert_eq!(decode_ansi("éà".as_bytes()), "éà");
        assert_eq!(decode_ansi(&[0x63, 0x61, 0x66, 0xE9]), "café", "Latin-1 fallback");
        assert_eq!(decode_ansi(b""), "");
    }
}

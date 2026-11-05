//! mIRC DLL compatible with FTPRush's `rushmirc.dll`, to drive UltraRushFTP from mIRC.
//!
//! This crate contains no code of its own: the exports (`LoadDll`, `UnloadDll`, `Invoke`,
//! `RushScript`, `RushCommand`, `EnableCallback`, `DisableCallback`) live in
//! `urftp_sys::mirc`, the only crate allowed to use `unsafe`, and are re-exported here so
//! that the `cdylib` exposes them.
//!
//! ```text
//! //echo -a $dll(rushmirc.dll, Invoke, RushApp.Version)
//! //dll rushmirc.dll RushScript RushApp.FTP.RAW('MySite', 'SITE WHO', RS_LOGIN);
//! ```
//!
//! Replies of `RAW` commands started by `RushScript` come back into mIRC as
//! `/echo -a (site) command`, then one line per reply line.
#![forbid(unsafe_code)]

#[cfg(windows)]
pub use urftp_sys::mirc::*;

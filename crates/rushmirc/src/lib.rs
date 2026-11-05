//! DLL mIRC compatible avec `rushmirc.dll` de FTPRush, pour piloter UltraRushFTP depuis mIRC.
//!
//! Exports (signature mIRC standard, convention `stdcall`) : `LoadDll`, `UnloadDll`,
//! `Invoke`, `RushScript`, `RushCommand`, `EnableCallback`, `DisableCallback` (ces deux
//! dernières manquaient aux exports de l'originale).
//!
//! ```text
//! //echo -a $dll(rushmirc.dll, Invoke, RushApp.Version)
//! //dll rushmirc.dll RushScript RushApp.FTP.RAW('MonSite', 'SITE WHO', RS_LOGIN);
//! ```
//!
//! Les réponses des commandes `RAW` lancées par `RushScript` reviennent dans mIRC sous la
//! forme `/echo -a (site) commande` puis une ligne par ligne de réponse.

pub use urftp_ipc as ipc;

#[cfg(windows)]
mod dll;

#[cfg(windows)]
pub use dll::*;

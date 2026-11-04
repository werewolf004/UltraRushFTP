//! DLL mIRC compatible avec `rushmirc.dll` de FTPRush.
//!
//! Exports prévus (signature mIRC standard, convention `stdcall`) :
//! `LoadDll`, `UnloadDll`, `Invoke`, `RushScript`, `RushCommand`,
//! `EnableCallback`, `DisableCallback` (ces deux dernières manquaient aux exports d'origine).
//!
//! L'implémentation viendra avec la crate `windows-sys` ; les constantes du protocole
//! sont partagées avec l'application via `urftp-ipc`.

pub use urftp_ipc as ipc;

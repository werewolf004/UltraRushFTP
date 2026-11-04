//! Constantes et (à venir) serveur du protocole d'échange avec `rushmirc.dll`.
//!
//! L'application crée une fenêtre de premier niveau cachée de classe [`WINDOW_CLASS`]
//! (la DLL la trouve via `FindWindow`), puis traite :
//! - `WM_EXCHANGE` : requête `Invoke` lue/écrite dans la mémoire partagée [`SHARED_MAP_NAME`]
//! - `WM_COPYDATA` avec [`COPYDATA_SCRIPT`] ou [`COPYDATA_COMMAND`]
//! - en retour, `WM_COPYDATA` vers la fenêtre de rappel avec `wParam` = [`CALLBACK_RAWCOMMAND`]

/// Classe de fenêtre recherchée par `rushmirc.dll`.
pub const WINDOW_CLASS: &str = "TfmRush";
/// Nom de la mémoire partagée application ↔ DLL.
pub const SHARED_MAP_NAME: &str = "FTPRush";
/// Nom de la mémoire partagée DLL → mIRC.
pub const MIRC_MAP_NAME: &str = "mIRC";
/// Taille des deux mémoires partagées.
pub const SHARED_MAP_SIZE: usize = 4096;

/// `WM_USER`.
pub const WM_USER: u32 = 0x0400;
/// Message envoyé par `Invoke`.
pub const WM_EXCHANGE: u32 = WM_USER + 300;
/// Message mIRC pour exécuter la commande placée dans la mémoire `mIRC`.
pub const WM_MCOMMAND: u32 = WM_USER + 200;

/// `COPYDATASTRUCT.dwData` d'un appel `RushScript`.
pub const COPYDATA_SCRIPT: usize = 1000;
/// `COPYDATASTRUCT.dwData` d'un appel `RushCommand`.
pub const COPYDATA_COMMAND: usize = 1001;
/// `wParam` du rappel portant le résultat d'une commande brute.
pub const CALLBACK_RAWCOMMAND: usize = 888;
/// `dwData` du rappel : succès / échec.
pub const CALLBACK_SUCCESS: usize = 1;
pub const CALLBACK_FAILED: usize = 0;

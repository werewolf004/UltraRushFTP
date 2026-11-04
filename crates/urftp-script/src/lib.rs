//! Interpréteur du Pascal Script de FTPRush.
//!
//! `rushmirc.dll` (fonction `RushScript`) envoie du code Pascal à l'application, par exemple :
//! `RushApp.FTP.Login('FTP1;FTP2','public',RS_BOOKMARK or RS_CHECKBM);`
//!
//! Prévu :
//! - analyseur lexical et syntaxique (appels, chaînes `'...'`, entiers, `or`/`and`, `if`,
//!   `begin`/`end`, variables, `inputQuery`, `ShowMessage`)
//! - évaluation des chemins `RushApp.*` pour `Invoke` (statistiques, `Containers.*`, `Version`…)
//! - table des constantes `RS_*` (noms repris de la documentation FTPRush)

/// Noms des constantes de drapeaux reconnues par l'API `RushApp.FTP.*` de FTPRush.
pub const RS_FLAG_NAMES: &[&str] = &[
    "RS_ALL", "RS_APPEND", "RS_ASYNC", "RS_BOOKMARK", "RS_BOTH", "RS_CHECKBM",
    "RS_CHECKLOGIN", "RS_CURRENT", "RS_DELEMPTYTABS", "RS_DIRDES", "RS_DIRSRC", "RS_DOWN",
    "RS_EXPR", "RS_FAIL", "RS_FILE", "RS_FOLDER", "RS_FULLPATH", "RS_FXP", "RS_GHOST",
    "RS_INVERT", "RS_LOGIN", "RS_LOGOUT", "RS_NOCIRCLE", "RS_NOMKD", "RS_NORMAL",
    "RS_NOSKIP", "RS_ONE", "RS_PAIR", "RS_POPUP", "RS_RECURSIVE", "RS_RELOGIN",
    "RS_SORTDATE", "RS_SORTDES", "RS_SORTSIZE", "RS_TRANSFER", "RS_UP", "RS_WAITING",
    "RS_WILD",
];

#[cfg(test)]
mod tests {
    #[test]
    fn flag_names_are_unique() {
        let mut v = super::RS_FLAG_NAMES.to_vec();
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), super::RS_FLAG_NAMES.len());
    }
}

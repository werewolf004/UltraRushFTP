//! Interpréteur du Pascal Script de FTPRush.
//!
//! `rushmirc.dll` (fonction `RushScript`) envoie du code Pascal à l'application, par exemple
//! `RushApp.FTP.Login('FTP1;FTP2','public',RS_BOOKMARK or RS_CHECKBM);`, et `Invoke` demande
//! une valeur : `RushApp.TotalDownloadSize`.
//!
//! L'interpréteur gère le sous-ensemble utilisé par les scripts FTPRush : variables,
//! chaînes, entiers, booléens, opérateurs (dont `or`/`and` bit à bit sur les drapeaux),
//! `if/then/else`, `while`, `repeat/until`, `for`, `begin/end`, `exit`, quelques fonctions
//! intégrées (`IntToStr`, `StrToInt`, `Length`, `Copy`, `Pos`, `UpperCase`, `ShowMessage`,
//! `InputQuery`…). Tout ce qui commence par `RushApp` est confié à l'hôte ([`Host`]),
//! c'est-à-dire à l'application.

mod interp;
mod lexer;
mod parser;

use std::fmt;

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

/// Valeur d'un drapeau `RS_*` (un bit par drapeau), insensible à la casse.
pub fn flag_value(name: &str) -> Option<i64> {
    RS_FLAG_NAMES.iter().position(|f| f.eq_ignore_ascii_case(name)).map(|i| 1i64 << i)
}

/// Le drapeau `name` est-il présent dans `flags` ?
pub fn has_flag(flags: i64, name: &str) -> bool {
    flag_value(name).is_some_and(|v| flags & v != 0)
}

/// Valeur manipulée par les scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Str(String),
    Bool(bool),
    Nil,
}

impl Value {
    /// Représentation texte (réponse d'`Invoke`, concaténation).
    pub fn to_text(&self) -> String {
        match self {
            Value::Int(i) => i.to_string(),
            Value::Str(s) => s.clone(),
            Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            Value::Nil => String::new(),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "entier",
            Value::Str(_) => "chaîne",
            Value::Bool(_) => "booléen",
            Value::Nil => "nil",
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Int(i) => Some(*i != 0),
            _ => None,
        }
    }
}

/// L'application qui exécute les scripts : reçoit les appels et lectures `RushApp.*`.
/// Les chemins sont transmis tels qu'écrits (`RushApp.FTP.Login`) ; comparez-les sans
/// tenir compte de la casse.
pub trait Host {
    /// Appel de procédure ou de fonction (`RushApp.FTP.Login(…)`, `ShowMessage`, `InputQuery`).
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, String>;
    /// Lecture d'une propriété (`RushApp.Version`, `RushApp.Containers.ContainerCount`).
    fn get(&mut self, name: &str) -> Result<Value, String>;
    /// Écriture d'une propriété (`RushApp.FTP.Splitter := '|'`).
    fn set(&mut self, name: &str, value: Value) -> Result<(), String>;
}

/// Erreur de script, avec son numéro de ligne (0 si inconnu).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 { write!(f, "ligne {} : {}", self.line, self.message) } else { f.write_str(&self.message) }
    }
}

impl std::error::Error for ScriptError {}

/// Exécute un script complet (`RushScript`).
pub fn run(source: &str, host: &mut dyn Host) -> Result<(), ScriptError> {
    let prog = parser::parse_program(lexer::tokenize(source)?)?;
    interp::Interpreter::new(host).run(&prog)
}

/// Évalue une seule expression et renvoie sa valeur (`Invoke`).
pub fn evaluate(expression: &str, host: &mut dyn Host) -> Result<Value, ScriptError> {
    let expr = parser::parse_expression(lexer::tokenize(expression.trim())?)?;
    let prog = [parser::Stmt::Assign(vec!["__resultat".into()], expr, 1)];
    let mut it = interp::Interpreter::new(host);
    it.run(&prog)?;
    Ok(it.take_var("__resultat").unwrap_or(Value::Nil))
}

/// Vérifie la syntaxe sans rien exécuter.
pub fn check(source: &str) -> Result<(), ScriptError> {
    parser::parse_program(lexer::tokenize(source)?).map(drop)
}

#[cfg(test)]
mod tests;

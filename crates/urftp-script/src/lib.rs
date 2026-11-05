//! Interpreter of the FTPRush Pascal Script.
//!
//! `rushmirc.dll` (function `RushScript`) sends Pascal code to the application, for example
//! `RushApp.FTP.Login('FTP1;FTP2','public',RS_BOOKMARK or RS_CHECKBM);`, and `Invoke` asks
//! for a value: `RushApp.TotalDownloadSize`.
//!
//! The interpreter handles the subset used by FTPRush scripts: variables, strings,
//! integers, booleans, operators (including bitwise `or`/`and` on flags),
//! `if/then/else`, `while`, `repeat/until`, `for`, `begin/end`, `exit`, a few built-in
//! functions (`IntToStr`, `StrToInt`, `Length`, `Copy`, `Pos`, `UpperCase`, `ShowMessage`,
//! `InputQuery`...). Everything starting with `RushApp` is handed to the host ([`Host`]),
//! that is to the application.

mod interp;
mod lexer;
mod parser;

/// Names of the flag constants known by the FTPRush `RushApp.FTP.*` API.
pub const RS_FLAG_NAMES: &[&str] = &[
    "RS_ALL",
    "RS_APPEND",
    "RS_ASYNC",
    "RS_BOOKMARK",
    "RS_BOTH",
    "RS_CHECKBM",
    "RS_CHECKLOGIN",
    "RS_CURRENT",
    "RS_DELEMPTYTABS",
    "RS_DIRDES",
    "RS_DIRSRC",
    "RS_DOWN",
    "RS_EXPR",
    "RS_FAIL",
    "RS_FILE",
    "RS_FOLDER",
    "RS_FULLPATH",
    "RS_FXP",
    "RS_GHOST",
    "RS_INVERT",
    "RS_LOGIN",
    "RS_LOGOUT",
    "RS_NOCIRCLE",
    "RS_NOMKD",
    "RS_NORMAL",
    "RS_NOSKIP",
    "RS_ONE",
    "RS_PAIR",
    "RS_POPUP",
    "RS_RECURSIVE",
    "RS_RELOGIN",
    "RS_SORTDATE",
    "RS_SORTDES",
    "RS_SORTSIZE",
    "RS_TRANSFER",
    "RS_UP",
    "RS_WAITING",
    "RS_WILD",
];

/// Value of an `RS_*` flag (one bit per flag), case-insensitive.
pub fn flag_value(name: &str) -> Option<i64> {
    RS_FLAG_NAMES.iter().position(|f| f.eq_ignore_ascii_case(name)).map(|i| 1i64 << i)
}

/// Is the flag `name` set in `flags`?
pub fn has_flag(flags: i64, name: &str) -> bool {
    flag_value(name).is_some_and(|v| flags & v != 0)
}

/// Value handled by scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Str(String),
    Bool(bool),
    Nil,
}

impl Value {
    /// Text representation (`Invoke` answer, concatenation).
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
            Value::Int(_) => "integer",
            Value::Str(_) => "string",
            Value::Bool(_) => "boolean",
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

/// The application running the scripts: receives the `RushApp.*` calls and reads. Paths
/// are passed as written (`RushApp.FTP.Login`); compare them case-insensitively.
pub trait Host {
    /// Procedure or function call (`RushApp.FTP.Login(…)`, `ShowMessage`, `InputQuery`).
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, HostError>;
    /// Property read (`RushApp.Version`, `RushApp.Containers.ContainerCount`).
    fn get(&mut self, name: &str) -> Result<Value, HostError>;
    /// Property write (`RushApp.FTP.Splitter := '|'`).
    fn set(&mut self, name: &str, value: Value) -> Result<(), HostError>;
}

/// Error reported by the host (the application) for a `RushApp.*` call or property.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    /// No such procedure or function.
    #[error("unknown function: {0}")]
    UnknownFunction(String),
    /// No such property.
    #[error("unknown property: {0}")]
    UnknownProperty(String),
    /// The property exists but cannot be assigned.
    #[error("read-only property: {0}")]
    ReadOnlyProperty(String),
    /// Wrong number of arguments.
    #[error("{name} expects at least {expected} argument(s), got {got}")]
    Arity {
        /// Called function.
        name: String,
        /// Minimum number of arguments.
        expected: usize,
        /// Number received.
        got: usize,
    },
    /// No site with this name in the site manager.
    #[error("unknown site: {0}")]
    UnknownSite(String),
    /// The site exists but is disabled (`RushApp.FTP.DisableSite`).
    #[error("disabled site: {0}")]
    DisabledSite(String),
    /// No bookmark with this name.
    #[error("bookmark \"{bookmark}\" not found{}", site.as_ref().map(|s| format!(" on site {s}")).unwrap_or_default())]
    UnknownBookmark {
        /// Requested bookmark.
        bookmark: String,
        /// Site searched, when known.
        site: Option<String>,
    },
    /// An argument has an invalid value.
    #[error("{function}: {reason}")]
    InvalidArgument {
        /// Called function.
        function: String,
        /// What is wrong.
        reason: String,
    },
}

/// Category of a script error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptErrorKind {
    /// The source cannot be tokenised or parsed.
    #[error("syntax error: {0}")]
    Syntax(String),
    /// Valid syntax that the interpreter does not support yet.
    #[error("not supported yet: {0}")]
    Unsupported(String),
    /// Type mismatch, bad argument to a built-in function...
    #[error("{0}")]
    Runtime(String),
    /// Integer division or modulo by zero.
    #[error("division by zero")]
    DivisionByZero,
    /// Too many statements executed: probable infinite loop.
    #[error("script stopped: too many statements (infinite loop?)")]
    StepLimit,
    /// Error returned by the application.
    #[error(transparent)]
    Host(#[from] HostError),
}

/// Script error with its line number (1-based; 0 when unknown).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {kind}")]
pub struct ScriptError {
    /// Line of the failing statement.
    pub line: usize,
    /// What went wrong.
    pub kind: ScriptErrorKind,
}

impl ScriptError {
    /// Error of kind `kind` at `line`.
    pub fn new(line: usize, kind: impl Into<ScriptErrorKind>) -> Self {
        Self { line, kind: kind.into() }
    }

    pub(crate) fn syntax(line: usize, message: impl Into<String>) -> Self {
        Self::new(line, ScriptErrorKind::Syntax(message.into()))
    }

    pub(crate) fn runtime(line: usize, message: impl Into<String>) -> Self {
        Self::new(line, ScriptErrorKind::Runtime(message.into()))
    }
}

/// Runs a complete script (`RushScript`).
pub fn run(source: &str, host: &mut dyn Host) -> Result<(), ScriptError> {
    let prog = parser::parse_program(lexer::tokenize(source)?)?;
    interp::Interpreter::new(host).run(&prog)
}

/// Evaluates a single expression and returns its value (`Invoke`).
pub fn evaluate(expression: &str, host: &mut dyn Host) -> Result<Value, ScriptError> {
    let expr = parser::parse_expression(lexer::tokenize(expression.trim())?)?;
    let prog = [parser::Stmt::Assign(vec!["__result".into()], expr, 1)];
    let mut it = interp::Interpreter::new(host);
    it.run(&prog)?;
    Ok(it.take_var("__result").unwrap_or(Value::Nil))
}

/// Checks the syntax without running anything.
pub fn check(source: &str) -> Result<(), ScriptError> {
    parser::parse_program(lexer::tokenize(source)?).map(drop)
}

#[cfg(test)]
mod tests;

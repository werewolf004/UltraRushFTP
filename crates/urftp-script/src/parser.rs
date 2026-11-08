//! Parsing: the Pascal Script used by FTPRush scripts.
//!
//! Program: optional `uses …;`, then `var`, `const`, `procedure` and `function`
//! declarations in any order, then statements (optional `begin … end`, optional final
//! dot). Statements: call, assignment `:=` (also to an index: `A[1] := x`), `if/then/else`,
//! `case … of`, `while/do`, `repeat/until`, `for … to|downto … do`, `break`, `continue`,
//! `exit`. Types may be written but are ignored: values carry their own type.

use std::collections::HashMap;

use crate::ScriptError;
use crate::lexer::{Tok, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Str(String),
    Bool(bool),
    Nil,
    /// Plain name or dotted path (`x`, `RushApp.Version`).
    Name(Vec<String>, usize),
    Call(Vec<String>, Vec<Expr>, usize),
    /// Array constructor `[a, b, c]`.
    Array(Vec<Expr>),
    /// `value[i]` or `value[i, j]`.
    Index(Box<Expr>, Vec<Expr>, usize),
    /// `value.Field` after an index or a call (`Items[0].Name`).
    Field(Box<Expr>, String, usize),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
}

/// Left side of an assignment: a variable or a property, with optional indexes.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub path: Vec<String>,
    pub index: Vec<Expr>,
}

impl Target {
    pub fn var(name: &str) -> Self {
        Self { path: vec![name.to_string()], index: Vec::new() }
    }
}

/// Label of a `case` branch: a value or a range `a..b`.
#[derive(Debug, Clone, PartialEq)]
pub enum Label {
    Value(Expr),
    Range(Expr, Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Block(Vec<Stmt>),
    Assign(Target, Expr, usize),
    Call(Vec<String>, Vec<Expr>, usize),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    Case { selector: Expr, arms: Vec<(Vec<Label>, Stmt)>, other: Option<Vec<Stmt>>, line: usize },
    While(Expr, Box<Stmt>),
    Repeat(Vec<Stmt>, Expr),
    For { var: String, from: Expr, to: Expr, down: bool, body: Box<Stmt>, line: usize },
    Break,
    Continue,
    Exit,
    Empty,
}

/// Parameter of a procedure; `var` parameters are passed by reference.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Lower-case name.
    pub name: String,
    pub by_ref: bool,
}

/// Procedure or function declared by the script.
#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    /// Name as written.
    pub name: String,
    pub params: Vec<Param>,
    /// A function returns `Result` (or the value assigned to its name).
    pub function: bool,
    /// Lower-case names of the local variables (`var` of the procedure).
    pub locals: Vec<String>,
    pub body: Vec<Stmt>,
}

/// A parsed script: its procedures and its main statements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    /// Procedures and functions by lower-case name.
    pub procs: HashMap<String, Proc>,
    pub main: Vec<Stmt>,
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn peek_at(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.pos + n).map(|t| &t.tok)
    }
    fn line(&self) -> usize {
        self.toks[self.pos].line
    }
    fn next(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }
    fn err<T>(&self, m: impl Into<String>) -> Result<T, ScriptError> {
        Err(ScriptError::syntax(self.line(), m))
    }
    fn is_kw(&self, k: &str) -> bool {
        matches!(self.peek(), Tok::Kw(w) if *w == k)
    }
    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Sym(w) if *w == s)
    }
    fn eat_kw(&mut self, k: &str) -> bool {
        let ok = self.is_kw(k);
        if ok {
            self.next();
        }
        ok
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        let ok = self.is_sym(s);
        if ok {
            self.next();
        }
        ok
    }
    fn expect_kw(&mut self, k: &str) -> Result<(), ScriptError> {
        if self.eat_kw(k) { Ok(()) } else { self.err(format!("\"{k}\" expected")) }
    }
    fn expect_sym(&mut self, s: &str) -> Result<(), ScriptError> {
        if self.eat_sym(s) { Ok(()) } else { self.err(format!("\"{s}\" expected")) }
    }
    fn ident(&mut self) -> Result<String, ScriptError> {
        match self.next() {
            Tok::Ident(s) => Ok(s),
            _ => {
                self.pos -= 1;
                self.err("identifier expected")
            }
        }
    }
    /// `..` of a range (two dot tokens).
    fn eat_range(&mut self) -> bool {
        let ok = self.is_sym(".") && self.peek_at(1) == Some(&Tok::Sym("."));
        if ok {
            self.next();
            self.next();
        }
        ok
    }

    // ------------------------------------------------------------------ program

    fn program(&mut self) -> Result<Program, ScriptError> {
        if self.eat_kw("uses") {
            loop {
                self.ident()?;
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(";")?;
        }
        let mut prog = Program::default();
        loop {
            if self.eat_kw("var") {
                self.var_names()?;
            } else if self.eat_kw("const") {
                prog.main.extend(self.constants()?);
            } else if self.is_kw("procedure") || self.is_kw("function") {
                let line = self.line();
                let p = self.procedure()?;
                if prog.procs.insert(p.name.to_ascii_lowercase(), p).is_some() {
                    return Err(ScriptError::syntax(line, "procedure or function declared twice"));
                }
            } else {
                break;
            }
        }
        prog.main.extend(self.statements(&[])?);
        self.eat_sym(".");
        if *self.peek() != Tok::Eof {
            return self.err("end of script expected");
        }
        Ok(prog)
    }

    /// Skips a type (`string`, `array of string`, `array[0..3] of integer`) up to one of
    /// `stops` outside brackets.
    fn skip_type(&mut self, stops: &[&str]) -> Result<(), ScriptError> {
        let mut depth = 0usize;
        loop {
            match self.peek() {
                Tok::Eof => return self.err("end of the type expected"),
                Tok::Sym(s) if depth == 0 && stops.contains(s) => return Ok(()),
                Tok::Sym("(" | "[") => depth += 1,
                Tok::Sym(")" | "]") => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.next();
        }
    }

    /// `a, b: string; c;` after `var` — the types are skipped. A name followed by `:=` or
    /// `(` starts the statements instead.
    fn var_names(&mut self) -> Result<Vec<String>, ScriptError> {
        let mut out = Vec::new();
        while matches!(self.peek(), Tok::Ident(_)) && matches!(self.peek_at(1), Some(Tok::Sym("," | ":" | ";"))) {
            loop {
                out.push(self.ident()?.to_ascii_lowercase());
                if !self.eat_sym(",") {
                    break;
                }
            }
            if self.eat_sym(":") {
                self.skip_type(&[";", "="])?;
            }
            if self.eat_sym("=") {
                // Initial value: `var n: integer = 5;` is read like an assignment by
                // `constants`; here it is simply skipped.
                self.expr()?;
            }
            self.expect_sym(";")?;
        }
        Ok(out)
    }

    /// `NAME = value;` (or `NAME: type = value;`) after `const`: assignments run first.
    fn constants(&mut self) -> Result<Vec<Stmt>, ScriptError> {
        let mut out = Vec::new();
        while matches!(self.peek(), Tok::Ident(_)) {
            let line = self.line();
            let name = self.ident()?;
            if self.eat_sym(":") {
                self.skip_type(&["="])?;
            }
            self.expect_sym("=")?;
            out.push(Stmt::Assign(Target::var(&name), self.expr()?, line));
            self.expect_sym(";")?;
        }
        Ok(out)
    }

    /// `procedure Name(A, B; var C);` or `function Name(A): type;`, its `var` and `const`
    /// sections, then `begin … end;`.
    fn procedure(&mut self) -> Result<Proc, ScriptError> {
        let function = self.is_kw("function");
        self.next();
        let name = self.ident()?;
        let mut params = Vec::new();
        if self.eat_sym("(") && !self.eat_sym(")") {
            loop {
                let by_ref = self.eat_kw("var");
                if !by_ref {
                    self.eat_kw("const");
                }
                loop {
                    params.push(Param { name: self.ident()?.to_ascii_lowercase(), by_ref });
                    if !self.eat_sym(",") {
                        break;
                    }
                }
                if self.eat_sym(":") {
                    self.skip_type(&[";", ")"])?;
                }
                if self.eat_sym(")") {
                    break;
                }
                self.expect_sym(";")?;
            }
        }
        if self.eat_sym(":") {
            self.skip_type(&[";"])?;
        }
        self.expect_sym(";")?;
        let mut locals = Vec::new();
        let mut body = Vec::new();
        loop {
            if self.eat_kw("var") {
                locals.extend(self.var_names()?);
            } else if self.eat_kw("const") {
                body.extend(self.constants()?);
            } else {
                break;
            }
        }
        if self.is_kw("procedure") || self.is_kw("function") {
            return Err(ScriptError::new(self.line(), crate::ScriptErrorKind::Unsupported("nested procedures".into())));
        }
        self.expect_kw("begin")?;
        body.extend(self.statements(&["end"])?);
        self.expect_kw("end")?;
        self.eat_sym(";");
        Ok(Proc { name, params, function, locals, body })
    }

    /// Statements separated by `;` until one of the ending keywords (or the end of the text).
    fn statements(&mut self, until: &[&str]) -> Result<Vec<Stmt>, ScriptError> {
        let mut out = Vec::new();
        loop {
            if *self.peek() == Tok::Eof || until.iter().any(|k| self.is_kw(k)) || self.is_sym(".") {
                return Ok(out);
            }
            if self.is_kw("procedure") || self.is_kw("function") {
                return self.err("procedures and functions must be declared before the statements");
            }
            let s = self.statement()?;
            if s != Stmt::Empty {
                out.push(s);
            }
            if !self.eat_sym(";") {
                if *self.peek() == Tok::Eof || until.iter().any(|k| self.is_kw(k)) || self.is_sym(".") {
                    return Ok(out);
                }
                return self.err("\";\" expected between two statements");
            }
        }
    }

    fn statement(&mut self) -> Result<Stmt, ScriptError> {
        let line = self.line();
        if self.eat_kw("begin") {
            let body = self.statements(&["end"])?;
            self.expect_kw("end")?;
            return Ok(Stmt::Block(body));
        }
        if self.eat_kw("if") {
            let cond = self.expr()?;
            self.expect_kw("then")?;
            let then = self.statement()?;
            let other = if self.eat_kw("else") { Some(Box::new(self.statement()?)) } else { None };
            return Ok(Stmt::If(cond, Box::new(then), other));
        }
        if self.eat_kw("case") {
            return self.case(line);
        }
        if self.eat_kw("while") {
            let cond = self.expr()?;
            self.expect_kw("do")?;
            return Ok(Stmt::While(cond, Box::new(self.statement()?)));
        }
        if self.eat_kw("repeat") {
            let body = self.statements(&["until"])?;
            self.expect_kw("until")?;
            return Ok(Stmt::Repeat(body, self.expr()?));
        }
        if self.eat_kw("for") {
            let var = self.ident()?;
            self.expect_sym(":=")?;
            let from = self.expr()?;
            let down = if self.eat_kw("downto") {
                true
            } else {
                self.expect_kw("to")?;
                false
            };
            let to = self.expr()?;
            self.expect_kw("do")?;
            return Ok(Stmt::For { var, from, to, down, body: Box::new(self.statement()?), line });
        }
        if self.eat_kw("exit") {
            return Ok(Stmt::Exit);
        }
        if self.eat_kw("break") {
            return Ok(Stmt::Break);
        }
        if self.eat_kw("continue") {
            return Ok(Stmt::Continue);
        }
        if !matches!(self.peek(), Tok::Ident(_)) {
            return Ok(Stmt::Empty);
        }
        let path = self.path()?;
        let mut index = Vec::new();
        while self.is_sym("[") {
            index.extend(self.index_list()?);
        }
        if self.eat_sym(":=") {
            return Ok(Stmt::Assign(Target { path, index }, self.expr()?, line));
        }
        if !index.is_empty() {
            return self.err("\":=\" expected after an index");
        }
        let args = if self.is_sym("(") { self.args()? } else { Vec::new() };
        Ok(Stmt::Call(path, args, line))
    }

    /// `case x of 1, 2: …; 3..5: …; else …; end` — labels of any type.
    fn case(&mut self, line: usize) -> Result<Stmt, ScriptError> {
        let selector = self.expr()?;
        self.expect_kw("of")?;
        let mut arms = Vec::new();
        while !self.is_kw("else") && !self.is_kw("end") {
            let mut labels = Vec::new();
            loop {
                let a = self.expr()?;
                labels.push(if self.eat_range() { Label::Range(a, self.expr()?) } else { Label::Value(a) });
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(":")?;
            arms.push((labels, self.statement()?));
            if !self.eat_sym(";") && !self.is_kw("else") && !self.is_kw("end") {
                return self.err("\";\" expected between two branches of case");
            }
        }
        let other = if self.eat_kw("else") { Some(self.statements(&["end"])?) } else { None };
        self.expect_kw("end")?;
        Ok(Stmt::Case { selector, arms, other, line })
    }

    fn path(&mut self) -> Result<Vec<String>, ScriptError> {
        let mut p = vec![self.ident()?];
        while self.is_sym(".") && matches!(self.peek_at(1), Some(Tok::Ident(_))) {
            self.next();
            p.push(self.ident()?);
        }
        Ok(p)
    }

    /// `[a, b]` after a value: one or more indexes.
    fn index_list(&mut self) -> Result<Vec<Expr>, ScriptError> {
        self.expect_sym("[")?;
        let mut out = vec![self.expr()?];
        while self.eat_sym(",") {
            out.push(self.expr()?);
        }
        self.expect_sym("]")?;
        Ok(out)
    }

    fn args(&mut self) -> Result<Vec<Expr>, ScriptError> {
        self.expect_sym("(")?;
        let mut out = Vec::new();
        if self.eat_sym(")") {
            return Ok(out);
        }
        loop {
            out.push(self.expr()?);
            if self.eat_sym(")") {
                return Ok(out);
            }
            self.expect_sym(",")?;
        }
    }

    // ------------------------------------------------------------------ expressions (Pascal precedence)

    pub fn expr(&mut self) -> Result<Expr, ScriptError> {
        let left = self.simple()?;
        let op = match self.peek() {
            Tok::Sym("=") => BinOp::Eq,
            Tok::Sym("<>") => BinOp::Ne,
            Tok::Sym("<") => BinOp::Lt,
            Tok::Sym("<=") => BinOp::Le,
            Tok::Sym(">") => BinOp::Gt,
            Tok::Sym(">=") => BinOp::Ge,
            Tok::Kw("in") => BinOp::In,
            _ => return Ok(left),
        };
        self.next();
        Ok(Expr::Binary(op, Box::new(left), Box::new(self.simple()?)))
    }

    fn simple(&mut self) -> Result<Expr, ScriptError> {
        let neg = if self.eat_sym("-") {
            true
        } else {
            self.eat_sym("+");
            false
        };
        let mut e = self.term()?;
        if neg {
            e = Expr::Unary(UnOp::Neg, Box::new(e));
        }
        loop {
            let op = match self.peek() {
                Tok::Sym("+") => BinOp::Add,
                Tok::Sym("-") => BinOp::Sub,
                Tok::Kw("or") => BinOp::Or,
                Tok::Kw("xor") => BinOp::Xor,
                _ => return Ok(e),
            };
            self.next();
            e = Expr::Binary(op, Box::new(e), Box::new(self.term()?));
        }
    }

    fn term(&mut self) -> Result<Expr, ScriptError> {
        let mut e = self.factor()?;
        loop {
            let op = match self.peek() {
                Tok::Sym("*") => BinOp::Mul,
                Tok::Sym("/") | Tok::Kw("div") => BinOp::Div,
                Tok::Kw("mod") => BinOp::Mod,
                Tok::Kw("and") => BinOp::And,
                _ => return Ok(e),
            };
            self.next();
            e = Expr::Binary(op, Box::new(e), Box::new(self.factor()?));
        }
    }

    fn factor(&mut self) -> Result<Expr, ScriptError> {
        let line = self.line();
        match self.peek().clone() {
            Tok::Int(v) => {
                self.next();
                Ok(Expr::Int(v))
            }
            Tok::Str(s) => {
                self.next();
                Ok(Expr::Str(s))
            }
            Tok::Kw("true") => {
                self.next();
                Ok(Expr::Bool(true))
            }
            Tok::Kw("false") => {
                self.next();
                Ok(Expr::Bool(false))
            }
            Tok::Kw("nil") => {
                self.next();
                Ok(Expr::Nil)
            }
            Tok::Kw("not") => {
                self.next();
                Ok(Expr::Unary(UnOp::Not, Box::new(self.factor()?)))
            }
            Tok::Sym("(") => {
                self.next();
                let e = self.expr()?;
                self.expect_sym(")")?;
                Ok(e)
            }
            Tok::Sym("[") => {
                self.next();
                let mut items = Vec::new();
                if !self.eat_sym("]") {
                    loop {
                        items.push(self.expr()?);
                        if self.eat_sym("]") {
                            break;
                        }
                        self.expect_sym(",")?;
                    }
                }
                Ok(Expr::Array(items))
            }
            Tok::Ident(_) => {
                let path = self.path()?;
                let mut e = if self.is_sym("(") { Expr::Call(path, self.args()?, line) } else { Expr::Name(path, line) };
                loop {
                    if self.is_sym("[") {
                        e = Expr::Index(Box::new(e), self.index_list()?, line);
                    } else if self.is_sym(".") && matches!(self.peek_at(1), Some(Tok::Ident(_))) {
                        self.next();
                        e = Expr::Field(Box::new(e), self.ident()?, line);
                    } else {
                        return Ok(e);
                    }
                }
            }
            _ => self.err("value expected"),
        }
    }
}

pub fn parse_program(toks: Vec<Token>) -> Result<Program, ScriptError> {
    Parser { toks, pos: 0 }.program()
}

/// A single expression (for `Invoke`), trailing `;` tolerated.
pub fn parse_expression(toks: Vec<Token>) -> Result<Expr, ScriptError> {
    let mut p = Parser { toks, pos: 0 };
    let e = p.expr()?;
    p.eat_sym(";");
    if *p.peek() != Tok::Eof {
        return p.err("a single expression expected");
    }
    Ok(e)
}

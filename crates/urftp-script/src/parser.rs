//! Parsing: the Pascal Script subset used by FTPRush scripts.
//!
//! Program: optional `uses …;`, optional `var` declarations, then statements (optional
//! `begin … end`, optional final dot). Statements: call, assignment `:=`, `if/then/else`,
//! `while/do`, `repeat/until`, `for … to|downto … do`, `exit`.

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
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Block(Vec<Stmt>),
    Assign(Vec<String>, Expr, usize),
    Call(Vec<String>, Vec<Expr>, usize),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    Repeat(Vec<Stmt>, Expr),
    For { var: String, from: Expr, to: Expr, down: bool, body: Box<Stmt>, line: usize },
    Exit,
    Empty,
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
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

    // ------------------------------------------------------------------ program

    fn program(&mut self) -> Result<Vec<Stmt>, ScriptError> {
        if self.eat_kw("uses") {
            loop {
                self.ident()?;
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(";")?;
        }
        while self.is_kw("var") || self.is_kw("const") {
            self.declarations()?;
        }
        if self.is_kw("procedure") || self.is_kw("function") {
            return Err(ScriptError::new(self.line(), crate::ScriptErrorKind::Unsupported("procedures and functions".into())));
        }
        let stmts = self.statements(&[])?;
        self.eat_sym(".");
        if *self.peek() != Tok::Eof {
            return self.err("end of script expected");
        }
        Ok(stmts)
    }

    /// `var a, b: string; c: integer;` — types are read then ignored.
    fn declarations(&mut self) -> Result<(), ScriptError> {
        let is_const = self.is_kw("const");
        self.next();
        while matches!(self.peek(), Tok::Ident(_)) {
            if is_const {
                // `const NAME = value;`: not used by common FTPRush scripts.
                return Err(ScriptError::new(self.line(), crate::ScriptErrorKind::Unsupported("constant declarations".into())));
            }
            loop {
                self.ident()?;
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(":")?;
            self.ident()?;
            self.expect_sym(";")?;
        }
        Ok(())
    }

    /// Statements separated by `;` until one of the ending keywords (or the end of the text).
    fn statements(&mut self, until: &[&str]) -> Result<Vec<Stmt>, ScriptError> {
        let mut out = Vec::new();
        loop {
            if *self.peek() == Tok::Eof || until.iter().any(|k| self.is_kw(k)) || self.is_sym(".") {
                return Ok(out);
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
        if !matches!(self.peek(), Tok::Ident(_)) {
            return Ok(Stmt::Empty);
        }
        let path = self.path()?;
        if self.eat_sym(":=") {
            return Ok(Stmt::Assign(path, self.expr()?, line));
        }
        let args = if self.is_sym("(") { self.args()? } else { Vec::new() };
        Ok(Stmt::Call(path, args, line))
    }

    fn path(&mut self) -> Result<Vec<String>, ScriptError> {
        let mut p = vec![self.ident()?];
        while self.is_sym(".") && matches!(self.toks.get(self.pos + 1).map(|t| &t.tok), Some(Tok::Ident(_))) {
            self.next();
            p.push(self.ident()?);
        }
        Ok(p)
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
            Tok::Ident(_) => {
                let path = self.path()?;
                if self.is_sym("(") { Ok(Expr::Call(path, self.args()?, line)) } else { Ok(Expr::Name(path, line)) }
            }
            _ => self.err("value expected"),
        }
    }
}

pub fn parse_program(toks: Vec<Token>) -> Result<Vec<Stmt>, ScriptError> {
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

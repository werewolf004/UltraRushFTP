//! Execution: variables, procedures, operators, built-in functions, calls to the host
//! (`RushApp.*`).

use std::collections::HashMap;
use std::sync::Arc;

use crate::parser::{BinOp, Expr, Label, Proc, Program, Stmt, UnOp};
use crate::{EventHandler, Host, HostError, ScriptError, ScriptErrorKind, Value, flag_value};

/// Maximum number of executed statements (protection against infinite loops).
const MAX_STEPS: u64 = 1_000_000;

/// Maximum depth of nested procedure calls (protection against endless recursion).
const MAX_DEPTH: usize = 64;

/// Variables of a running procedure.
struct Frame {
    vars: HashMap<String, Value>,
    /// Lower-case name of the running function: assigning to it sets `Result`.
    function: Option<String>,
}

pub struct Interpreter<'h> {
    host: &'h mut dyn Host,
    program: Arc<Program>,
    globals: HashMap<String, Value>,
    frames: Vec<Frame>,
    steps: u64,
    /// Line of the current statement (for errors without a precise position).
    line: usize,
}

/// Interruption of the normal flow.
enum Flow {
    Exit,
    Break,
    Continue,
    Error(ScriptError),
}

impl From<ScriptError> for Flow {
    fn from(e: ScriptError) -> Self {
        Flow::Error(e)
    }
}

fn err(line: usize, m: impl Into<String>) -> ScriptError {
    ScriptError::runtime(line, m)
}

fn host_err(line: usize, e: HostError) -> ScriptError {
    ScriptError::new(line, e)
}

/// Does a loop stop after this pass of its body?
fn loop_flow(r: Result<(), Flow>) -> Result<bool, Flow> {
    match r {
        Ok(()) | Err(Flow::Continue) => Ok(false),
        Err(Flow::Break) => Ok(true),
        Err(e) => Err(e),
    }
}

/// Position in an array (0-based) or a string (1-based) as a `usize`.
fn position(i: &Value, len: usize, base: i64, line: usize) -> Result<usize, ScriptError> {
    let Value::Int(n) = i else {
        return Err(err(line, format!("an index must be an integer ({} received)", i.type_name())));
    };
    usize::try_from(n - base)
        .ok()
        .filter(|&p| p < len)
        .ok_or_else(|| err(line, format!("index {n} out of bounds ({base}..{})", len as i64 - 1 + base)))
}

fn get_index(v: Value, idx: &[Value], line: usize) -> Result<Value, ScriptError> {
    let Some((i, rest)) = idx.split_first() else { return Ok(v) };
    match v {
        Value::Array(mut items) => {
            let p = position(i, items.len(), 0, line)?;
            get_index(items.swap_remove(p), rest, line)
        }
        Value::Str(s) if rest.is_empty() => {
            let chars: Vec<char> = s.chars().collect();
            Ok(Value::Str(chars[position(i, chars.len(), 1, line)?].to_string()))
        }
        other => Err(err(line, format!("{} cannot be indexed", other.type_name()))),
    }
}

fn set_index(v: Value, idx: &[Value], new: Value, line: usize) -> Result<Value, ScriptError> {
    let Some((i, rest)) = idx.split_first() else { return Ok(new) };
    match v {
        Value::Array(mut items) => {
            let p = position(i, items.len(), 0, line)?;
            let old = std::mem::replace(&mut items[p], Value::Nil);
            items[p] = set_index(old, rest, new, line)?;
            Ok(Value::Array(items))
        }
        Value::Str(s) if rest.is_empty() => {
            let mut chars: Vec<char> = s.chars().collect();
            let p = position(i, chars.len(), 1, line)?;
            let text = new.to_text();
            let mut it = text.chars();
            let (Some(c), None) = (it.next(), it.next()) else {
                return Err(err(line, "a single character is expected"));
            };
            chars[p] = c;
            Ok(Value::Str(chars.into_iter().collect()))
        }
        other => Err(err(line, format!("{} cannot be indexed", other.type_name()))),
    }
}

impl<'h> Interpreter<'h> {
    pub fn new(host: &'h mut dyn Host, program: Arc<Program>) -> Self {
        Self { host, program, globals: HashMap::new(), frames: Vec::new(), steps: 0, line: 0 }
    }

    pub fn take_var(&mut self, name: &str) -> Option<Value> {
        self.globals.remove(&name.to_ascii_lowercase())
    }

    fn finish(&self, r: Result<(), Flow>) -> Result<(), ScriptError> {
        match r {
            Ok(()) | Err(Flow::Exit) => Ok(()),
            Err(Flow::Break | Flow::Continue) => Err(err(self.line, "break or continue outside a loop")),
            Err(Flow::Error(mut e)) => {
                if e.line == 0 {
                    e.line = self.line;
                }
                Err(e)
            }
        }
    }

    /// Runs the main statements.
    pub fn run(&mut self) -> Result<(), ScriptError> {
        let prog = Arc::clone(&self.program);
        let r = self.block(&prog.main);
        self.finish(r)
    }

    /// Runs the procedure `name` as an event handler (`Sender` is nil).
    pub fn fire(&mut self, name: &str) -> Result<(), ScriptError> {
        let prog = Arc::clone(&self.program);
        let Some(p) = prog.procs.get(&name.to_ascii_lowercase()) else {
            return Err(err(0, format!("unknown procedure: {name}")));
        };
        let args: Vec<Expr> = p.params.iter().map(|_| Expr::Nil).collect();
        let r = self.call_user(p, &args, 0).map(drop);
        self.finish(r)
    }

    fn block(&mut self, stmts: &[Stmt]) -> Result<(), Flow> {
        for s in stmts {
            self.stmt(s)?;
        }
        Ok(())
    }

    fn tick(&mut self, line: usize) -> Result<(), Flow> {
        if line > 0 {
            self.line = line;
        }
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(ScriptError::new(line, ScriptErrorKind::StepLimit).into());
        }
        Ok(())
    }

    // ------------------------------------------------------------------ variables

    fn var(&self, lower: &str) -> Option<&Value> {
        self.frames.last().and_then(|f| f.vars.get(lower)).or_else(|| self.globals.get(lower))
    }

    /// Sets a variable: local when the procedure has it (or nothing global has this name),
    /// `Result` when it is the name of the running function, global otherwise.
    fn assign(&mut self, name: &str, v: Value) {
        let lower = name.to_ascii_lowercase();
        if let Some(f) = self.frames.last_mut() {
            if let Some(slot) = f.vars.get_mut(&lower) {
                *slot = v;
                return;
            }
            if f.function.as_deref() == Some(lower.as_str()) {
                f.vars.insert("result".into(), v);
                return;
            }
            if !self.globals.contains_key(&lower) {
                f.vars.insert(lower, v);
                return;
            }
        }
        self.globals.insert(lower, v);
    }

    // ------------------------------------------------------------------ statements

    fn stmt(&mut self, s: &Stmt) -> Result<(), Flow> {
        match s {
            Stmt::Empty => Ok(()),
            Stmt::Exit => Err(Flow::Exit),
            Stmt::Break => Err(Flow::Break),
            Stmt::Continue => Err(Flow::Continue),
            Stmt::Block(b) => self.block(b),
            Stmt::Assign(target, e, line) => {
                self.tick(*line)?;
                if target.path.len() > 1 {
                    if !target.index.is_empty() {
                        return Err(err(*line, "an indexed property of the application cannot be assigned").into());
                    }
                    let v = self.event_or_value(e)?;
                    return self.host.set(&target.path.join("."), v).map_err(|e| host_err(*line, e).into());
                }
                let name = &target.path[0];
                if target.index.is_empty() {
                    let v = self.eval(e)?;
                    self.assign(name, v);
                    return Ok(());
                }
                let idx = target.index.iter().map(|i| self.eval(i)).collect::<Result<Vec<_>, _>>()?;
                let v = self.eval(e)?;
                let current = self.var(&name.to_ascii_lowercase()).cloned().unwrap_or(Value::Nil);
                let new = set_index(current, &idx, v, *line)?;
                self.assign(name, new);
                Ok(())
            }
            Stmt::Call(path, args, line) => {
                self.tick(*line)?;
                self.call(path, args, *line)?;
                Ok(())
            }
            Stmt::If(c, then, other) => {
                if self.truth(c)? {
                    self.stmt(then)
                } else if let Some(o) = other {
                    self.stmt(o)
                } else {
                    Ok(())
                }
            }
            Stmt::Case { selector, arms, other, line } => {
                self.tick(*line)?;
                let v = self.eval(selector)?;
                for (labels, body) in arms {
                    for label in labels {
                        if self.label_matches(&v, label, *line)? {
                            return self.stmt(body);
                        }
                    }
                }
                match other {
                    Some(o) => self.block(o),
                    None => Ok(()),
                }
            }
            Stmt::While(c, body) => {
                while self.truth(c)? {
                    self.tick(0)?;
                    if loop_flow(self.stmt(body))? {
                        break;
                    }
                }
                Ok(())
            }
            Stmt::Repeat(body, c) => loop {
                self.tick(0)?;
                if loop_flow(self.block(body))? || self.truth(c)? {
                    return Ok(());
                }
            },
            Stmt::For { var, from, to, down, body, line } => {
                let a = self.int(from, *line)?;
                let b = self.int(to, *line)?;
                let range: Box<dyn Iterator<Item = i64>> = if *down { Box::new((b..=a).rev()) } else { Box::new(a..=b) };
                for i in range {
                    self.tick(*line)?;
                    self.assign(var, Value::Int(i));
                    if loop_flow(self.stmt(body))? {
                        break;
                    }
                }
                Ok(())
            }
        }
    }

    fn label_matches(&mut self, v: &Value, label: &Label, line: usize) -> Result<bool, Flow> {
        let ok = |r: Result<Value, ScriptErrorKind>| -> Result<bool, Flow> {
            match r {
                Ok(Value::Bool(b)) => Ok(b),
                Ok(_) => Ok(false),
                Err(k) => Err(ScriptError::new(line, k).into()),
            }
        };
        match label {
            Label::Value(e) => {
                let x = self.eval(e)?;
                ok(binary(BinOp::Eq, v.clone(), x))
            }
            Label::Range(a, b) => {
                let (lo, hi) = (self.eval(a)?, self.eval(b)?);
                Ok(ok(binary(BinOp::Ge, v.clone(), lo))? && ok(binary(BinOp::Le, v.clone(), hi))?)
            }
        }
    }

    /// The value assigned to a property: a procedure of the script given to an event
    /// (`RushApp.UI.OnSwitchLayout := Handler`), or any value.
    fn event_or_value(&mut self, e: &Expr) -> Result<Value, Flow> {
        if let Expr::Name(p, _) = e
            && p.len() == 1
            && self.var(&p[0].to_ascii_lowercase()).is_none()
            && let Some(proc) = self.program.procs.get(&p[0].to_ascii_lowercase())
        {
            return Ok(Value::Event(EventHandler { program: Arc::clone(&self.program), name: proc.name.clone() }));
        }
        self.eval(e)
    }

    fn truth(&mut self, e: &Expr) -> Result<bool, Flow> {
        match self.eval(e)? {
            Value::Bool(b) => Ok(b),
            Value::Int(i) => Ok(i != 0),
            other => Err(err(line_of(e), format!("non-boolean condition ({})", other.type_name())).into()),
        }
    }

    fn int(&mut self, e: &Expr, line: usize) -> Result<i64, Flow> {
        match self.eval(e)? {
            Value::Int(i) => Ok(i),
            other => Err(err(line, format!("integer expected ({} received)", other.type_name())).into()),
        }
    }

    // ------------------------------------------------------------------ expressions

    fn eval(&mut self, e: &Expr) -> Result<Value, Flow> {
        Ok(match e {
            Expr::Int(i) => Value::Int(*i),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Nil => Value::Nil,
            Expr::Name(path, line) => self.name(path, *line)?,
            Expr::Call(path, args, line) => self.call(path, args, *line)?,
            Expr::Array(items) => Value::Array(items.iter().map(|i| self.eval(i)).collect::<Result<_, _>>()?),
            Expr::Index(x, idx, line) => {
                let v = self.eval(x)?;
                let idx = idx.iter().map(|i| self.eval(i)).collect::<Result<Vec<_>, _>>()?;
                get_index(v, &idx, *line)?
            }
            Expr::Field(x, field, line) => match self.eval(x)? {
                Value::Record(fields) => fields
                    .into_iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(field))
                    .map(|(_, v)| v)
                    .ok_or_else(|| err(*line, format!("no field {field}")))?,
                other => return Err(err(*line, format!("{} has no field {field}", other.type_name())).into()),
            },
            Expr::Unary(op, x) => {
                let v = self.eval(x)?;
                match (op, v) {
                    (UnOp::Neg, Value::Int(i)) => Value::Int(-i),
                    (UnOp::Not, Value::Bool(b)) => Value::Bool(!b),
                    (UnOp::Not, Value::Int(i)) => Value::Int(!i),
                    (_, v) => return Err(err(line_of(x), format!("operation impossible on {}", v.type_name())).into()),
                }
            }
            Expr::Binary(op, a, b) => {
                let line = line_of(a);
                let (x, y) = (self.eval(a)?, self.eval(b)?);
                binary(*op, x, y).map_err(|k| ScriptError::new(line, k))?
            }
        })
    }

    fn name(&mut self, path: &[String], line: usize) -> Result<Value, Flow> {
        if path.len() == 1 {
            let lower = path[0].to_ascii_lowercase();
            if let Some(v) = self.var(&lower) {
                return Ok(v.clone());
            }
            if let Some(f) = flag_value(&path[0]) {
                return Ok(Value::Int(f));
            }
            // Procedure, function or built-in function called without parentheses.
            let prog = Arc::clone(&self.program);
            if let Some(p) = prog.procs.get(&lower) {
                return self.call_user(p, &[], line);
            }
            if let Some(v) = self.builtin(&lower, &[], &[], line)? {
                return Ok(v);
            }
        }
        self.host.get(&path.join(".")).map_err(|e| host_err(line, e).into())
    }

    fn call(&mut self, path: &[String], args: &[Expr], line: usize) -> Result<Value, Flow> {
        if path.len() == 1 {
            let lower = path[0].to_ascii_lowercase();
            let prog = Arc::clone(&self.program);
            if let Some(p) = prog.procs.get(&lower) {
                return self.call_user(p, args, line);
            }
            if lower == "setlength" {
                return self.set_length(args, line);
            }
        }
        let values = args.iter().map(|a| self.eval(a)).collect::<Result<Vec<_>, _>>()?;
        if path.len() == 1
            && let Some(v) = self.builtin(&path[0].to_ascii_lowercase(), &values, args, line)?
        {
            return Ok(v);
        }
        self.host.call(&path.join("."), &values).map_err(|e| host_err(line, e).into())
    }

    /// Calls a procedure of the script; `var` parameters are copied back to the caller's
    /// variables when it returns.
    fn call_user(&mut self, p: &Proc, args: &[Expr], line: usize) -> Result<Value, Flow> {
        if self.frames.len() >= MAX_DEPTH {
            return Err(err(line, format!("{}: too many nested calls (endless recursion?)", p.name)).into());
        }
        if args.len() != p.params.len() {
            return Err(err(line, format!("{} expects {} argument(s), {} received", p.name, p.params.len(), args.len())).into());
        }
        let mut vars = HashMap::new();
        let mut refs = Vec::new();
        for (param, a) in p.params.iter().zip(args) {
            let v = if param.by_ref {
                let Expr::Name(target, _) = a else {
                    return Err(err(line, format!("{}: the argument of var parameter {} must be a variable", p.name, param.name)).into());
                };
                if target.len() != 1 {
                    return Err(err(line, format!("{}: the argument of var parameter {} must be a variable", p.name, param.name)).into());
                }
                refs.push((param.name.clone(), target[0].clone()));
                self.var(&target[0].to_ascii_lowercase()).cloned().unwrap_or(Value::Nil)
            } else {
                self.eval(a)?
            };
            vars.insert(param.name.clone(), v);
        }
        for l in &p.locals {
            vars.entry(l.clone()).or_insert(Value::Nil);
        }
        if p.function {
            vars.insert("result".into(), Value::Nil);
        }
        self.frames.push(Frame { vars, function: p.function.then(|| p.name.to_ascii_lowercase()) });
        let r = self.block(&p.body);
        let mut frame = self.frames.pop().ok_or_else(|| err(line, "call stack empty"))?;
        match r {
            Ok(()) | Err(Flow::Exit) => {}
            Err(Flow::Break | Flow::Continue) => return Err(err(self.line, "break or continue outside a loop").into()),
            Err(e) => return Err(e),
        }
        for (param, caller) in refs {
            let v = frame.vars.remove(&param).unwrap_or(Value::Nil);
            self.assign(&caller, v);
        }
        Ok(if p.function { frame.vars.remove("result").unwrap_or(Value::Nil) } else { Value::Nil })
    }

    /// `SetLength(variable, n)`: resizes an array (new items are nil) or a string.
    fn set_length(&mut self, args: &[Expr], line: usize) -> Result<Value, Flow> {
        let [Expr::Name(target, _), len] = args else {
            return Err(err(line, "SetLength expects a variable and a length").into());
        };
        if target.len() != 1 {
            return Err(err(line, "SetLength: the 1st argument must be a variable").into());
        }
        let n = usize::try_from(self.int(len, line)?).map_err(|_| err(line, "SetLength: negative length"))?;
        let new = match self.var(&target[0].to_ascii_lowercase()).cloned() {
            Some(Value::Str(s)) => Value::Str(s.chars().chain(std::iter::repeat(' ')).take(n).collect()),
            Some(Value::Array(mut items)) => {
                items.resize(n, Value::Nil);
                Value::Array(items)
            }
            _ => Value::Array(vec![Value::Nil; n]),
        };
        self.assign(&target[0], new);
        Ok(Value::Nil)
    }

    /// Built-in functions of Pascal Script; `None` when the name is not one of them.
    fn builtin(&mut self, name: &str, v: &[Value], raw: &[Expr], line: usize) -> Result<Option<Value>, Flow> {
        let arity = |n: usize| -> Result<(), Flow> {
            if v.len() == n { Ok(()) } else { Err(err(line, format!("{name} expects {n} argument(s), {} received", v.len())).into()) }
        };
        let s = |i: usize| v.get(i).map(Value::to_text).unwrap_or_default();
        let n = |i: usize| -> Result<i64, Flow> {
            match v.get(i) {
                Some(Value::Int(x)) => Ok(*x),
                _ => Err(err(line, format!("{name}: argument {} must be an integer", i + 1)).into()),
            }
        };
        Ok(Some(match name {
            "inttostr" => {
                arity(1)?;
                Value::Str(n(0)?.to_string())
            }
            "strtoint" => {
                arity(1)?;
                Value::Int(s(0).trim().parse().map_err(|_| err(line, format!("\"{}\" is not an integer", s(0))))?)
            }
            "strtointdef" => {
                arity(2)?;
                Value::Int(s(0).trim().parse().unwrap_or(n(1)?))
            }
            "length" => {
                arity(1)?;
                match &v[0] {
                    Value::Array(items) => Value::Int(items.len() as i64),
                    other => Value::Int(other.to_text().chars().count() as i64),
                }
            }
            "low" => {
                arity(1)?;
                Value::Int(if matches!(v[0], Value::Array(_)) { 0 } else { 1 })
            }
            "high" => {
                arity(1)?;
                match &v[0] {
                    Value::Array(items) => Value::Int(items.len() as i64 - 1),
                    other => Value::Int(other.to_text().chars().count() as i64),
                }
            }
            "uppercase" => {
                arity(1)?;
                Value::Str(s(0).to_uppercase())
            }
            "lowercase" => {
                arity(1)?;
                Value::Str(s(0).to_lowercase())
            }
            "trim" => {
                arity(1)?;
                Value::Str(s(0).trim().to_string())
            }
            "copy" => {
                arity(3)?;
                let (start, count) = (n(1)?.max(1) as usize - 1, n(2)?.max(0) as usize);
                Value::Str(s(0).chars().skip(start).take(count).collect())
            }
            "pos" => {
                arity(2)?;
                let (sub, hay) = (s(0), s(1));
                Value::Int(hay.find(&sub).map_or(0, |b| hay[..b].chars().count() as i64 + 1))
            }
            "chr" => {
                arity(1)?;
                let code = u32::try_from(n(0)?).ok().and_then(char::from_u32).ok_or_else(|| err(line, "Chr: invalid character code"))?;
                Value::Str(code.to_string())
            }
            "ord" => {
                arity(1)?;
                match &v[0] {
                    Value::Bool(b) => Value::Int(i64::from(*b)),
                    Value::Int(i) => Value::Int(*i),
                    other => Value::Int(other.to_text().chars().next().map_or(0, |c| i64::from(u32::from(c)))),
                }
            }
            "abs" => {
                arity(1)?;
                Value::Int(n(0)?.wrapping_abs())
            }
            "odd" => {
                arity(1)?;
                Value::Bool(n(0)? % 2 != 0)
            }
            "showmessage" => {
                arity(1)?;
                self.host.call("ShowMessage", v).map_err(|e| host_err(line, e))?;
                Value::Nil
            }
            "inputquery" => {
                // InputQuery(title, prompt, variable): the variable receives the input.
                arity(3)?;
                let Some(Expr::Name(target, _)) = raw.get(2).filter(|e| matches!(e, Expr::Name(p, _) if p.len() == 1)) else {
                    return Err(err(line, "InputQuery: the 3rd argument must be a variable").into());
                };
                match self.host.call("InputQuery", v).map_err(|e| host_err(line, e))? {
                    Value::Str(answer) => {
                        self.assign(&target[0], Value::Str(answer));
                        Value::Bool(true)
                    }
                    _ => Value::Bool(false),
                }
            }
            _ => return Ok(None),
        }))
    }
}

fn line_of(e: &Expr) -> usize {
    match e {
        Expr::Name(_, l) | Expr::Call(_, _, l) | Expr::Index(_, _, l) | Expr::Field(_, _, l) => *l,
        Expr::Unary(_, x) => line_of(x),
        Expr::Binary(_, a, _) => line_of(a),
        _ => 0,
    }
}

fn binary(op: BinOp, x: Value, y: Value) -> Result<Value, ScriptErrorKind> {
    use Value::*;
    let tn = |a: &Value, b: &Value| format!("{} and {}", a.type_name(), b.type_name());
    Ok(match (op, x, y) {
        (BinOp::Add, Int(a), Int(b)) => Int(a.wrapping_add(b)),
        (BinOp::Add, a @ (Str(_) | Int(_)), b @ (Str(_) | Int(_))) => Str(a.to_text() + &b.to_text()),
        (BinOp::Add, Array(mut a), Array(b)) => {
            a.extend(b);
            Array(a)
        }
        (BinOp::Sub, Int(a), Int(b)) => Int(a.wrapping_sub(b)),
        (BinOp::Mul, Int(a), Int(b)) => Int(a.wrapping_mul(b)),
        (BinOp::Div | BinOp::Mod, Int(_), Int(0)) => return Err(ScriptErrorKind::DivisionByZero),
        (BinOp::Div, Int(a), Int(b)) => Int(a / b),
        (BinOp::Mod, Int(a), Int(b)) => Int(a % b),
        (BinOp::And, Bool(a), Bool(b)) => Bool(a && b),
        (BinOp::Or, Bool(a), Bool(b)) => Bool(a || b),
        (BinOp::Xor, Bool(a), Bool(b)) => Bool(a ^ b),
        // On integers: bitwise operations (`RS_BOOKMARK or RS_CHECKBM`).
        (BinOp::And, Int(a), Int(b)) => Int(a & b),
        (BinOp::Or, Int(a), Int(b)) => Int(a | b),
        (BinOp::Xor, Int(a), Int(b)) => Int(a ^ b),
        (BinOp::In, a, Array(items)) => Bool(items.contains(&a)),
        (BinOp::Eq, a, b) => Bool(a == b),
        (BinOp::Ne, a, b) => Bool(a != b),
        (cmp @ (BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge), a, b) => {
            let o = match (&a, &b) {
                (Int(x), Int(y)) => x.cmp(y),
                (Str(x), Str(y)) => x.cmp(y),
                _ => return Err(ScriptErrorKind::Runtime(format!("cannot compare {}", tn(&a, &b)))),
            };
            Bool(match cmp {
                BinOp::Lt => o.is_lt(),
                BinOp::Le => o.is_le(),
                BinOp::Gt => o.is_gt(),
                _ => o.is_ge(),
            })
        }
        (_, a, b) => return Err(ScriptErrorKind::Runtime(format!("operation impossible between {}", tn(&a, &b)))),
    })
}

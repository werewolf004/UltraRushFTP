//! Exécution : variables, opérateurs, fonctions intégrées, appels vers l'hôte (`RushApp.*`).

use std::collections::HashMap;

use crate::parser::{BinOp, Expr, Stmt, UnOp};
use crate::{Host, HostError, ScriptError, ScriptErrorKind, Value, flag_value};

/// Nombre maximal d'instructions exécutées (protection contre les boucles infinies).
const MAX_STEPS: u64 = 1_000_000;

pub struct Interpreter<'h> {
    host: &'h mut dyn Host,
    vars: HashMap<String, Value>,
    steps: u64,
    /// Ligne de l'instruction en cours (pour les erreurs sans position précise).
    line: usize,
}

/// Interruption du déroulement normal.
enum Flow {
    Exit,
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

impl<'h> Interpreter<'h> {
    pub fn new(host: &'h mut dyn Host) -> Self {
        Self { host, vars: HashMap::new(), steps: 0, line: 0 }
    }

    pub fn take_var(&mut self, name: &str) -> Option<Value> {
        self.vars.remove(&name.to_ascii_lowercase())
    }

    pub fn run(&mut self, prog: &[Stmt]) -> Result<(), ScriptError> {
        match self.block(prog) {
            Ok(()) | Err(Flow::Exit) => Ok(()),
            Err(Flow::Error(mut e)) => {
                if e.line == 0 {
                    e.line = self.line;
                }
                Err(e)
            }
        }
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

    fn stmt(&mut self, s: &Stmt) -> Result<(), Flow> {
        match s {
            Stmt::Empty => Ok(()),
            Stmt::Exit => Err(Flow::Exit),
            Stmt::Block(b) => self.block(b),
            Stmt::Assign(path, e, line) => {
                self.tick(*line)?;
                let v = self.eval(e)?;
                if path.len() == 1 {
                    self.vars.insert(path[0].to_ascii_lowercase(), v);
                    Ok(())
                } else {
                    self.host.set(&path.join("."), v).map_err(|e| host_err(*line, e).into())
                }
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
            Stmt::While(c, body) => {
                while self.truth(c)? {
                    self.tick(0)?;
                    self.stmt(body)?;
                }
                Ok(())
            }
            Stmt::Repeat(body, c) => loop {
                self.tick(0)?;
                self.block(body)?;
                if self.truth(c)? {
                    return Ok(());
                }
            },
            Stmt::For { var, from, to, down, body, line } => {
                let a = self.int(from, *line)?;
                let b = self.int(to, *line)?;
                let name = var.to_ascii_lowercase();
                let range: Box<dyn Iterator<Item = i64>> = if *down { Box::new((b..=a).rev()) } else { Box::new(a..=b) };
                for i in range {
                    self.tick(*line)?;
                    self.vars.insert(name.clone(), Value::Int(i));
                    self.stmt(body)?;
                }
                Ok(())
            }
        }
    }

    fn truth(&mut self, e: &Expr) -> Result<bool, Flow> {
        match self.eval(e)? {
            Value::Bool(b) => Ok(b),
            Value::Int(i) => Ok(i != 0),
            other => Err(err(line_of(e), format!("condition non booléenne ({})", other.type_name())).into()),
        }
    }

    fn int(&mut self, e: &Expr, line: usize) -> Result<i64, Flow> {
        match self.eval(e)? {
            Value::Int(i) => Ok(i),
            other => Err(err(line, format!("entier attendu ({} reçu)", other.type_name())).into()),
        }
    }

    fn eval(&mut self, e: &Expr) -> Result<Value, Flow> {
        Ok(match e {
            Expr::Int(i) => Value::Int(*i),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Nil => Value::Nil,
            Expr::Name(path, line) => self.name(path, *line)?,
            Expr::Call(path, args, line) => self.call(path, args, *line)?,
            Expr::Unary(op, x) => {
                let v = self.eval(x)?;
                match (op, v) {
                    (UnOp::Neg, Value::Int(i)) => Value::Int(-i),
                    (UnOp::Not, Value::Bool(b)) => Value::Bool(!b),
                    (UnOp::Not, Value::Int(i)) => Value::Int(!i),
                    (_, v) => return Err(err(line_of(x), format!("opération impossible sur {}", v.type_name())).into()),
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
            if let Some(v) = self.vars.get(&lower) {
                return Ok(v.clone());
            }
            if let Some(f) = flag_value(&path[0]) {
                return Ok(Value::Int(f));
            }
            // Fonction intégrée appelée sans parenthèses.
            if let Some(v) = self.builtin(&lower, &[], &[], line)? {
                return Ok(v);
            }
        }
        self.host.get(&path.join(".")).map_err(|e| host_err(line, e).into())
    }

    fn call(&mut self, path: &[String], args: &[Expr], line: usize) -> Result<Value, Flow> {
        let values = args.iter().map(|a| self.eval(a)).collect::<Result<Vec<_>, _>>()?;
        if path.len() == 1
            && let Some(v) = self.builtin(&path[0].to_ascii_lowercase(), &values, args, line)?
        {
            return Ok(v);
        }
        self.host.call(&path.join("."), &values).map_err(|e| host_err(line, e).into())
    }

    /// Fonctions intégrées du Pascal Script ; `None` si le nom n'en est pas une.
    fn builtin(&mut self, name: &str, v: &[Value], raw: &[Expr], line: usize) -> Result<Option<Value>, Flow> {
        let arity = |n: usize| -> Result<(), Flow> {
            if v.len() == n { Ok(()) } else { Err(err(line, format!("{name} attend {n} argument(s), {} reçu(s)", v.len())).into()) }
        };
        let s = |i: usize| v.get(i).map(Value::to_text).unwrap_or_default();
        let n = |i: usize| -> Result<i64, Flow> {
            match v.get(i) {
                Some(Value::Int(x)) => Ok(*x),
                _ => Err(err(line, format!("{name} : argument {} entier attendu", i + 1)).into()),
            }
        };
        Ok(Some(match name {
            "inttostr" => {
                arity(1)?;
                Value::Str(n(0)?.to_string())
            }
            "strtoint" => {
                arity(1)?;
                Value::Int(s(0).trim().parse().map_err(|_| err(line, format!("« {} » n'est pas un entier", s(0))))?)
            }
            "strtointdef" => {
                arity(2)?;
                Value::Int(s(0).trim().parse().unwrap_or(n(1)?))
            }
            "length" => {
                arity(1)?;
                Value::Int(s(0).chars().count() as i64)
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
            "showmessage" => {
                arity(1)?;
                self.host.call("ShowMessage", v).map_err(|e| host_err(line, e))?;
                Value::Nil
            }
            "inputquery" => {
                // InputQuery(titre, invite, variable) : la variable reçoit la saisie.
                arity(3)?;
                let Some(Expr::Name(target, _)) = raw.get(2).filter(|e| matches!(e, Expr::Name(p, _) if p.len() == 1)) else {
                    return Err(err(line, "InputQuery : le 3e argument doit être une variable").into());
                };
                match self.host.call("InputQuery", v).map_err(|e| host_err(line, e))? {
                    Value::Str(answer) => {
                        self.vars.insert(target[0].to_ascii_lowercase(), Value::Str(answer));
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
        Expr::Name(_, l) | Expr::Call(_, _, l) => *l,
        Expr::Unary(_, x) => line_of(x),
        Expr::Binary(_, a, _) => line_of(a),
        _ => 0,
    }
}

fn binary(op: BinOp, x: Value, y: Value) -> Result<Value, ScriptErrorKind> {
    use Value::*;
    let tn = |a: &Value, b: &Value| format!("{} et {}", a.type_name(), b.type_name());
    Ok(match (op, x, y) {
        (BinOp::Add, Int(a), Int(b)) => Int(a.wrapping_add(b)),
        (BinOp::Add, a @ (Str(_) | Int(_)), b @ (Str(_) | Int(_))) => Str(a.to_text() + &b.to_text()),
        (BinOp::Sub, Int(a), Int(b)) => Int(a.wrapping_sub(b)),
        (BinOp::Mul, Int(a), Int(b)) => Int(a.wrapping_mul(b)),
        (BinOp::Div | BinOp::Mod, Int(_), Int(0)) => return Err(ScriptErrorKind::DivisionByZero),
        (BinOp::Div, Int(a), Int(b)) => Int(a / b),
        (BinOp::Mod, Int(a), Int(b)) => Int(a % b),
        (BinOp::And, Bool(a), Bool(b)) => Bool(a && b),
        (BinOp::Or, Bool(a), Bool(b)) => Bool(a || b),
        (BinOp::Xor, Bool(a), Bool(b)) => Bool(a ^ b),
        // Sur des entiers : opérations bit à bit (`RS_BOOKMARK or RS_CHECKBM`).
        (BinOp::And, Int(a), Int(b)) => Int(a & b),
        (BinOp::Or, Int(a), Int(b)) => Int(a | b),
        (BinOp::Xor, Int(a), Int(b)) => Int(a ^ b),
        (BinOp::Eq, a, b) => Bool(a == b),
        (BinOp::Ne, a, b) => Bool(a != b),
        (cmp @ (BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge), a, b) => {
            let o = match (&a, &b) {
                (Int(x), Int(y)) => x.cmp(y),
                (Str(x), Str(y)) => x.cmp(y),
                _ => return Err(ScriptErrorKind::Runtime(format!("comparaison impossible entre {}", tn(&a, &b)))),
            };
            Bool(match cmp {
                BinOp::Lt => o.is_lt(),
                BinOp::Le => o.is_le(),
                BinOp::Gt => o.is_gt(),
                _ => o.is_ge(),
            })
        }
        (_, a, b) => return Err(ScriptErrorKind::Runtime(format!("opération impossible entre {}", tn(&a, &b)))),
    })
}

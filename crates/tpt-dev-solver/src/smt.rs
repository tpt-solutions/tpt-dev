//! A reader for a subset of SMT-LIB 2 over real arithmetic (QF_LRA).
//!
//! Supported commands: `set-logic`, `set-info`, `set-option` (ignored),
//! `declare-fun` and `declare-const` (Real only), `assert`, `check-sat`,
//! `get-model`, `push`, `pop` and `exit`.
//!
//! Supported terms: numerals and decimals, `true`/`false`, declared
//! variables, `+`, `-`, `*` and `/` where one side is constant, `<=`, `<`,
//! `>=`, `>`, `=`, `not`, `and`, `or` and `=>`.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::rational::Rational;
use crate::solver::{Solver, Status};
use crate::term::{Formula, Linear, Var};

/// An error in a script. The message says what was wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    /// Human-readable description.
    pub message: String,
}

fn err<T>(message: impl Into<String>) -> Result<T, ScriptError> {
    Err(ScriptError {
        message: message.into(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

fn parse_sexps(src: &str) -> Result<Vec<Sexp>, ScriptError> {
    let mut stack: Vec<Vec<Sexp>> = vec![Vec::new()];
    let mut chars = src.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            ';' => {
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '\n' {
                        break;
                    }
                }
            }
            '(' => {
                chars.next();
                stack.push(Vec::new());
            }
            ')' => {
                chars.next();
                if stack.len() == 1 {
                    return err("unbalanced ')'");
                }
                let list = stack.pop().unwrap_or_default();
                if let Some(top) = stack.last_mut() {
                    top.push(Sexp::List(list));
                }
            }
            _ => {
                let mut atom = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '(' || c == ')' || c == ';' {
                        break;
                    }
                    atom.push(c);
                    chars.next();
                }
                if let Some(top) = stack.last_mut() {
                    top.push(Sexp::Atom(atom));
                }
            }
        }
    }
    if stack.len() != 1 {
        return err("unbalanced '('");
    }
    Ok(stack.pop().unwrap_or_default())
}

/// A parsed term: a real-valued linear expression or a Boolean formula.
#[derive(Clone, Debug)]
enum Val {
    Real(Linear),
    Bool(Formula),
}

/// A session: a solver plus the names of its variables, which persist across
/// calls to [`Session::run`].
///
/// ```
/// use tpt_dev_solver::Session;
///
/// let mut session = Session::new();
/// let out = session.run(
///     "(declare-fun x () Real)
///      (assert (and (> x 1.5) (< x 2)))
///      (check-sat)
///      (get-model)",
/// ).unwrap();
/// assert!(out.starts_with("sat\n(model\n"));
/// ```
pub struct Session {
    solver: Solver,
    names: BTreeMap<String, Var>,
    last: Option<Status>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// Creates an empty session.
    pub const fn new() -> Self {
        Self {
            solver: Solver::new(),
            names: BTreeMap::new(),
            last: None,
        }
    }

    /// The underlying solver.
    pub fn solver(&self) -> &Solver {
        &self.solver
    }

    /// Runs a script and returns the text it prints, one response per line.
    /// Stops at the first error, or at `exit`.
    pub fn run(&mut self, script: &str) -> Result<String, ScriptError> {
        let cmds = parse_sexps(script)?;
        let mut out = String::new();
        for cmd in &cmds {
            if !self.command(cmd, &mut out)? {
                break;
            }
        }
        Ok(out)
    }

    /// Runs one command. Returns `false` on `exit`.
    fn command(&mut self, cmd: &Sexp, out: &mut String) -> Result<bool, ScriptError> {
        let Sexp::List(items) = cmd else {
            return err("a command must be a list");
        };
        let Some((head, args)) = items.split_first() else {
            return err("empty command");
        };
        let Sexp::Atom(name) = head else {
            return err("command name must be a symbol");
        };
        match name.as_str() {
            "set-logic" | "set-info" | "set-option" => {}
            "declare-fun" => {
                // (declare-fun x () Real)
                match args {
                    [Sexp::Atom(x), Sexp::List(params), Sexp::Atom(sort)] if params.is_empty() => {
                        self.declare(x, sort)?;
                    }
                    _ => return err("unsupported declare-fun (only nullary Real functions)"),
                }
            }
            "declare-const" => match args {
                [Sexp::Atom(x), Sexp::Atom(sort)] => self.declare(x, sort)?,
                _ => return err("malformed declare-const"),
            },
            "assert" => match args {
                [t] => match self.term(t)? {
                    Val::Bool(f) => self.solver.assert(f),
                    Val::Real(_) => return err("assert needs a Boolean term"),
                },
                _ => return err("assert takes one term"),
            },
            "check-sat" => {
                let status = self.solver.check().map_err(|_| ScriptError {
                    message: "unknown: arithmetic overflow".to_string(),
                })?;
                self.last = Some(status);
                out.push_str(match status {
                    Status::Sat => "sat\n",
                    Status::Unsat => "unsat\n",
                });
            }
            "get-model" => {
                if self.last != Some(Status::Sat) {
                    return err("get-model requires a preceding sat result");
                }
                out.push_str("(model\n");
                let mut decls: Vec<(String, Var)> =
                    self.names.iter().map(|(k, v)| (k.clone(), *v)).collect();
                decls.sort_by_key(|(_, v)| v.index());
                for (n, v) in decls {
                    let val = self.solver.value(v).ok_or_else(|| ScriptError {
                        message: "no value for variable".to_string(),
                    })?;
                    out.push_str(&format!(
                        "  (define-fun {n} () Real {})\n",
                        format_rational(val)
                    ));
                }
                out.push_str(")\n");
            }
            "push" | "pop" => {
                let n = match args {
                    [] => 1,
                    [Sexp::Atom(k)] => k.parse::<usize>().map_err(|_| ScriptError {
                        message: "push/pop count must be a numeral".to_string(),
                    })?,
                    _ => return err("push/pop takes at most one numeral"),
                };
                for _ in 0..n {
                    if name == "push" {
                        self.solver.push();
                    } else if !self.solver.pop() {
                        return err("pop without matching push");
                    }
                }
                self.last = None;
            }
            "exit" => return Ok(false),
            other => return err(format!("unsupported command '{other}'")),
        }
        Ok(true)
    }

    fn declare(&mut self, name: &str, sort: &str) -> Result<(), ScriptError> {
        if sort != "Real" {
            return err(format!("unsupported sort '{sort}' (only Real)"));
        }
        if self.names.contains_key(name) {
            return err(format!("'{name}' is already declared"));
        }
        let v = self.solver.declare(name);
        self.names.insert(name.to_string(), v);
        Ok(())
    }

    fn term(&mut self, t: &Sexp) -> Result<Val, ScriptError> {
        match t {
            Sexp::Atom(a) => self.atom_term(a),
            Sexp::List(items) => {
                let Some((head, args)) = items.split_first() else {
                    return err("empty term");
                };
                let Sexp::Atom(op) = head else {
                    return err("term head must be a symbol");
                };
                self.apply(op, args)
            }
        }
    }

    fn atom_term(&mut self, a: &str) -> Result<Val, ScriptError> {
        match a {
            "true" => Ok(Val::Bool(self.solver.arena_mut().constant(true))),
            "false" => Ok(Val::Bool(self.solver.arena_mut().constant(false))),
            _ => {
                if let Some(&v) = self.names.get(a) {
                    return Ok(Val::Real(Linear::var(v)));
                }
                match parse_decimal(a) {
                    Some(r) => Ok(Val::Real(Linear::constant(r))),
                    None => err(format!("unknown symbol '{a}'")),
                }
            }
        }
    }

    fn apply(&mut self, op: &str, args: &[Sexp]) -> Result<Val, ScriptError> {
        match op {
            "+" | "-" | "*" | "/" => {
                let reals = args
                    .iter()
                    .map(|a| self.real_term(a))
                    .collect::<Result<Vec<_>, _>>()?;
                self.arith(op, &reals)
            }
            "<=" | "<" | ">=" | ">" => {
                let reals = args
                    .iter()
                    .map(|a| self.real_term(a))
                    .collect::<Result<Vec<_>, _>>()?;
                if reals.len() < 2 {
                    return err(format!("'{op}' needs at least two arguments"));
                }
                let mut parts = Vec::new();
                for pair in reals.windows(2) {
                    let (a, b) = (&pair[0], &pair[1]);
                    let f = match op {
                        "<=" => self.solver.arena_mut().le(a, b),
                        "<" => self.solver.arena_mut().lt(a, b),
                        ">=" => self.solver.arena_mut().ge(a, b),
                        _ => self.solver.arena_mut().gt(a, b),
                    };
                    parts.push(f.ok_or_else(overflow)?);
                }
                Ok(Val::Bool(self.solver.arena_mut().and(parts)))
            }
            "=" => {
                if args.len() < 2 {
                    return err("'=' needs at least two arguments");
                }
                let first = self.term(&args[0])?;
                let mut parts = Vec::new();
                match first {
                    Val::Real(a) => {
                        for arg in &args[1..] {
                            let b = self.real_term(arg)?;
                            parts.push(self.solver.arena_mut().eq(&a, &b).ok_or_else(overflow)?);
                        }
                    }
                    Val::Bool(a) => {
                        for arg in &args[1..] {
                            let Val::Bool(b) = self.term(arg)? else {
                                return err("'=' mixes Boolean and Real terms");
                            };
                            // a <=> b, as (a and b) or (not a and not b).
                            let ar = self.solver.arena_mut();
                            let na = ar.not(a);
                            let nb = ar.not(b);
                            let both = ar.and(vec![a, b]);
                            let neither = ar.and(vec![na, nb]);
                            parts.push(ar.or(vec![both, neither]));
                        }
                    }
                }
                Ok(Val::Bool(self.solver.arena_mut().and(parts)))
            }
            "not" => match args {
                [t] => match self.term(t)? {
                    Val::Bool(f) => Ok(Val::Bool(self.solver.arena_mut().not(f))),
                    Val::Real(_) => err("'not' needs a Boolean argument"),
                },
                _ => err("'not' takes one argument"),
            },
            "and" | "or" => {
                let fs = self.bool_args(args)?;
                let ar = self.solver.arena_mut();
                Ok(Val::Bool(if op == "and" { ar.and(fs) } else { ar.or(fs) }))
            }
            "=>" => {
                let mut fs = self.bool_args(args)?;
                if fs.len() < 2 {
                    return err("'=>' needs at least two arguments");
                }
                // Right-associative: a => (b => c).
                let mut acc = fs.pop().ok_or_else(|| ScriptError {
                    message: "'=>' needs at least two arguments".to_string(),
                })?;
                while let Some(a) = fs.pop() {
                    acc = self.solver.arena_mut().implies(a, acc);
                }
                Ok(Val::Bool(acc))
            }
            other => err(format!("unsupported operator '{other}'")),
        }
    }

    fn bool_args(&mut self, args: &[Sexp]) -> Result<Vec<Formula>, ScriptError> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            match self.term(a)? {
                Val::Bool(f) => out.push(f),
                Val::Real(_) => return err("expected a Boolean argument"),
            }
        }
        Ok(out)
    }

    fn real_term(&mut self, t: &Sexp) -> Result<Linear, ScriptError> {
        match self.term(t)? {
            Val::Real(r) => Ok(r),
            Val::Bool(_) => err("expected a Real argument"),
        }
    }

    fn arith(&mut self, op: &str, args: &[Linear]) -> Result<Val, ScriptError> {
        let Some((first, rest)) = args.split_first() else {
            return err(format!("'{op}' needs arguments"));
        };
        let mut acc = first.clone();
        match op {
            "-" if rest.is_empty() => {
                acc = acc.scale(Rational::from_int(-1)).ok_or_else(overflow)?
            }
            "+" => {
                for x in rest {
                    acc = acc.checked_add(x).ok_or_else(overflow)?;
                }
            }
            "-" => {
                for x in rest {
                    acc = acc.checked_sub(x).ok_or_else(overflow)?;
                }
            }
            "*" => {
                for x in rest {
                    acc = multiply(&acc, x).ok_or_else(|| ScriptError {
                        message: "nonlinear multiplication is not supported".to_string(),
                    })?;
                }
            }
            _ => {
                // "/": divide by a nonzero constant.
                for x in rest {
                    if !x.terms().is_empty() || x.constant_term().is_zero() {
                        return err("division is only supported by a nonzero constant");
                    }
                    acc = acc
                        .scale(
                            Rational::ONE
                                .checked_div(x.constant_term())
                                .ok_or_else(overflow)?,
                        )
                        .ok_or_else(overflow)?;
                }
            }
        }
        Ok(Val::Real(acc))
    }
}

/// Product of two linear expressions, when one of them is constant.
fn multiply(a: &Linear, b: &Linear) -> Option<Linear> {
    if a.terms().is_empty() {
        b.scale(a.constant_term())
    } else if b.terms().is_empty() {
        a.scale(b.constant_term())
    } else {
        None
    }
}

fn overflow() -> ScriptError {
    ScriptError {
        message: "unknown: arithmetic overflow".to_string(),
    }
}

/// Parses `123` or `1.25` into an exact rational.
fn parse_decimal(s: &str) -> Option<Rational> {
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mut num: i128 = 0;
    let mut den: i128 = 1;
    for b in int_part.bytes() {
        num = num.checked_mul(10)?.checked_add(i128::from(b - b'0'))?;
    }
    for b in frac_part.bytes() {
        num = num.checked_mul(10)?.checked_add(i128::from(b - b'0'))?;
        den = den.checked_mul(10)?;
    }
    Rational::new(num, den)
}

/// Formats a rational in SMT-LIB style: `5`, `(/ 3 2)`, `(- 5)`, `(- (/ 3 2))`.
pub(crate) fn format_rational(r: Rational) -> String {
    let neg = r.is_negative();
    let n = r.num().unsigned_abs();
    let body = if r.is_integer() {
        format!("{n}")
    } else {
        format!("(/ {n} {})", r.den())
    };
    if neg { format!("(- {body})") } else { body }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parses_decimals_and_formats() {
        assert_eq!(parse_decimal("1.25"), Rational::new(5, 4));
        assert_eq!(parse_decimal("3"), Some(Rational::from_int(3)));
        assert_eq!(parse_decimal("x"), None);
        assert_eq!(
            format_rational(Rational::new(-3, 2).unwrap()),
            "(- (/ 3 2))"
        );
        assert_eq!(format_rational(Rational::from_int(-4)), "(- 4)");
    }

    #[test]
    fn sexp_comments_and_nesting() {
        let v = parse_sexps("(a ; comment\n (b c)) d").unwrap();
        assert_eq!(v.len(), 2);
        assert!(parse_sexps("(a").is_err());
        assert!(parse_sexps("a)").is_err());
    }

    #[test]
    fn rejects_nonlinear_and_unknown() {
        let mut s = Session::new();
        let r = s.run("(declare-fun x () Real)(assert (> (* x x) 1))");
        assert!(r.err().unwrap().message.contains("nonlinear"));
        let r = s.run("(assert (> y 1))");
        assert!(r.err().unwrap().message.contains("unknown symbol"));
    }

    #[test]
    fn push_pop_and_exit() {
        let mut s = Session::new();
        let out = s
            .run(
                "(declare-const x Real)
                 (assert (> x 0))
                 (push)
                 (assert (< x 0))
                 (check-sat)
                 (pop)
                 (check-sat)
                 (exit)
                 (check-sat)",
            )
            .unwrap();
        assert_eq!(out, "unsat\nsat\n");
    }

    #[test]
    fn model_output() {
        let mut s = Session::new();
        let out = s
            .run(
                "(declare-fun x () Real)(declare-fun y () Real)
                 (assert (= (+ x y) 10))
                 (assert (= (- x y) 4))
                 (check-sat)
                 (get-model)",
            )
            .unwrap();
        assert_eq!(
            out,
            "sat\n(model\n  (define-fun x () Real 7)\n  (define-fun y () Real 3)\n)\n"
        );
    }

    #[test]
    fn get_model_needs_sat() {
        let mut s = Session::new();
        assert!(s.run("(get-model)").is_err());
    }
}

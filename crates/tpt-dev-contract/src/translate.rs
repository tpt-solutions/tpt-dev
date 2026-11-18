//! Translation of the supported Rust expression subset into solver terms.
//!
//! Supported: integer literals, `true` and `false`, names, unary `-` and `!`,
//! binary `+`, `-`, `*` (one side constant), comparisons, `&&`, `||`, and
//! parentheses. Anything else is reported as unsupported. The caller then
//! falls back to a run-time check.

use std::collections::HashMap;

use syn::{BinOp, Expr, Lit, UnOp};
use tpt_dev_solver::{Arena, Formula, Linear, Rational};

/// A translated value: a linear number, or a Boolean formula.
#[derive(Clone, Debug)]
pub(crate) enum Val {
    Num(Linear),
    Bool(Formula),
}

/// Why an expression cannot be translated.
pub(crate) type Failure = String;

/// Translates expressions against an arena, with names bound in `env`.
pub(crate) struct Translator<'a> {
    pub(crate) arena: &'a mut Arena,
    pub(crate) env: HashMap<String, Val>,
}

fn num(v: Val) -> Result<Linear, Failure> {
    match v {
        Val::Num(l) => Ok(l),
        Val::Bool(_) => Err("expected a number".into()),
    }
}

fn boolean(v: Val) -> Result<Formula, Failure> {
    match v {
        Val::Bool(f) => Ok(f),
        Val::Num(_) => Err("expected a boolean".into()),
    }
}

fn overflow() -> Failure {
    "arithmetic does not fit the solver's range".into()
}

impl Translator<'_> {
    /// Translates one expression.
    pub(crate) fn expr(&mut self, e: &Expr) -> Result<Val, Failure> {
        match e {
            Expr::Paren(p) => self.expr(&p.expr),
            Expr::Group(g) => self.expr(&g.expr),
            Expr::Lit(l) => match &l.lit {
                Lit::Int(i) => {
                    let v: i128 = i
                        .base10_parse()
                        .map_err(|_| String::from("integer literal does not fit"))?;
                    Ok(Val::Num(Linear::constant(Rational::from_int(v))))
                }
                Lit::Bool(b) => Ok(Val::Bool(self.arena.constant(b.value))),
                _ => Err("unsupported literal".into()),
            },
            Expr::Path(p) => {
                let name = p
                    .path
                    .get_ident()
                    .ok_or_else(|| String::from("paths are not supported"))?
                    .to_string();
                self.env
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| format!("unknown name `{name}`"))
            }
            Expr::Unary(u) => {
                let inner = self.expr(&u.expr)?;
                match u.op {
                    UnOp::Neg(_) => Ok(Val::Num(
                        num(inner)?
                            .scale(Rational::from_int(-1))
                            .ok_or_else(overflow)?,
                    )),
                    UnOp::Not(_) => Ok(Val::Bool(self.arena.not(boolean(inner)?))),
                    _ => Err("unsupported unary operator".into()),
                }
            }
            Expr::Binary(b) => self.binary(&b.left, &b.op, &b.right),
            _ => Err("unsupported expression".into()),
        }
    }

    fn binary(&mut self, left: &Expr, op: &BinOp, right: &Expr) -> Result<Val, Failure> {
        let l = self.expr(left)?;
        let r = self.expr(right)?;
        match op {
            BinOp::Add(_) => Ok(Val::Num(
                num(l)?.checked_add(&num(r)?).ok_or_else(overflow)?,
            )),
            BinOp::Sub(_) => Ok(Val::Num(
                num(l)?.checked_sub(&num(r)?).ok_or_else(overflow)?,
            )),
            BinOp::Mul(_) => {
                let (a, b) = (num(l)?, num(r)?);
                // Multiplication is linear only when one side is constant.
                if a.terms().is_empty() {
                    Ok(Val::Num(b.scale(a.constant_term()).ok_or_else(overflow)?))
                } else if b.terms().is_empty() {
                    Ok(Val::Num(a.scale(b.constant_term()).ok_or_else(overflow)?))
                } else {
                    Err("non-linear multiplication".into())
                }
            }
            BinOp::Lt(_)
            | BinOp::Le(_)
            | BinOp::Gt(_)
            | BinOp::Ge(_)
            | BinOp::Eq(_)
            | BinOp::Ne(_) => {
                let (a, b) = (num(l)?, num(r)?);
                let f = match op {
                    BinOp::Lt(_) => self.arena.lt(&a, &b),
                    BinOp::Le(_) => self.arena.le(&a, &b),
                    BinOp::Gt(_) => self.arena.gt(&a, &b),
                    BinOp::Ge(_) => self.arena.ge(&a, &b),
                    BinOp::Eq(_) => self.arena.eq(&a, &b),
                    _ => self.arena.eq(&a, &b).map(|f| self.arena.not(f)),
                };
                Ok(Val::Bool(f.ok_or_else(overflow)?))
            }
            BinOp::And(_) => Ok(Val::Bool(self.arena.and(vec![boolean(l)?, boolean(r)?]))),
            BinOp::Or(_) => Ok(Val::Bool(self.arena.or(vec![boolean(l)?, boolean(r)?]))),
            _ => Err("unsupported operator".into()),
        }
    }
}

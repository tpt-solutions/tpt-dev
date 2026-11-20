//! Symbolic execution of a function body into its return points.
//!
//! The body is walked statement by statement. A `let` binds a translated value.
//! An `if` splits the walk into two paths, each with its condition added to the
//! path condition. A `return` or a final expression ends a path and records
//! `(path condition, value)`. Blocks scope their `let` bindings.

use std::collections::HashMap;

use syn::{Block, Expr, Pat, Stmt};
use tpt_dev_solver::Formula;

use crate::translate::{Failure, Translator, Val};

/// The most return points a body may have before the proof is abandoned.
const MAX_RETURNS: usize = 64;

/// Work still to do on a path. The stack is processed from the top.
#[derive(Clone)]
enum Item<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
    /// Restores the bindings of the enclosing scope when a block ends.
    Restore(HashMap<String, Val>),
}

/// The name bound by a `let` pattern: `x` or `x: T`.
fn binding_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(p) => Some(p.ident.to_string()),
        Pat::Type(pt) => binding_name(&pt.pat),
        _ => None,
    }
}

/// Pushes the statements of a block so that the first one is on top.
fn push_block<'a>(stack: &mut Vec<Item<'a>>, block: &'a Block) {
    for stmt in block.stmts.iter().rev() {
        stack.push(Item::Stmt(stmt));
    }
}

/// Every return point of the body, as `(path condition, value)` pairs.
///
/// The conditions of the returned pairs cover every path through the body, so
/// the value of the function is determined by exactly one of them.
pub(crate) fn return_points(
    tr: &mut Translator<'_>,
    block: &Block,
) -> Result<Vec<(Formula, Val)>, Failure> {
    let pc = tr.arena.constant(true);
    let env = tr.env.clone();
    let mut stack = Vec::new();
    push_block(&mut stack, block);
    let mut out = Vec::new();
    walk(tr, stack, pc, env, &mut out)?;
    Ok(out)
}

/// Walks one path to its end, recording its return points in `out`.
fn walk<'a>(
    tr: &mut Translator<'_>,
    mut stack: Vec<Item<'a>>,
    pc: Formula,
    mut env: HashMap<String, Val>,
    out: &mut Vec<(Formula, Val)>,
) -> Result<(), Failure> {
    while let Some(item) = stack.pop() {
        let ended = match item {
            Item::Restore(saved) => {
                env = saved;
                false
            }
            Item::Stmt(stmt) => match stmt {
                Stmt::Local(local) => {
                    let name = binding_name(&local.pat).ok_or("unsupported `let` pattern")?;
                    let init = local.init.as_ref().ok_or("`let` without a value")?;
                    if init.diverge.is_some() {
                        return Err("`let else` is not supported".into());
                    }
                    tr.env = env.clone();
                    let v = tr.expr(&init.expr)?;
                    env.insert(name, v);
                    false
                }
                Stmt::Expr(e, semi) => {
                    let is_last = stack.iter().all(|i| matches!(i, Item::Restore(_)));
                    step(tr, e, semi.is_some(), is_last, &mut stack, pc, &env, out)?
                }
                _ => {
                    return Err(
                        "only `let`, `if`, `return` and blocks are supported in the body".into(),
                    );
                }
            },
            Item::Expr(e) => {
                let is_last = stack.iter().all(|i| matches!(i, Item::Restore(_)));
                step(tr, e, false, is_last, &mut stack, pc, &env, out)?
            }
        };
        if ended {
            return Ok(());
        }
    }
    Err("a path through the body ends without a value".into())
}

/// Handles one expression statement. Returns `true` when the path has ended.
#[allow(clippy::too_many_arguments)]
fn step<'a>(
    tr: &mut Translator<'_>,
    e: &'a Expr,
    semi: bool,
    is_last: bool,
    stack: &mut Vec<Item<'a>>,
    pc: Formula,
    env: &HashMap<String, Val>,
    out: &mut Vec<(Formula, Val)>,
) -> Result<bool, Failure> {
    match e {
        Expr::Return(r) => {
            let value = r.expr.as_deref().ok_or("`return` without a value")?;
            tr.env = env.clone();
            let v = tr.expr(value)?;
            record(out, pc, v)?;
            Ok(true)
        }
        Expr::If(ifx) => {
            tr.env = env.clone();
            let c = match tr.expr(&ifx.cond)? {
                Val::Bool(f) => f,
                Val::Num(_) => return Err("an `if` condition must be a boolean".into()),
            };
            let not_c = tr.arena.not(c);
            let then_pc = tr.arena.and(vec![pc, c]);
            let else_pc = tr.arena.and(vec![pc, not_c]);

            // The then branch, then the rest of this path.
            let mut then_stack: Vec<Item<'a>> = stack.clone();
            then_stack.push(Item::Restore(env.clone()));
            push_block(&mut then_stack, &ifx.then_branch);
            walk(tr, then_stack, then_pc, env.clone(), out)?;

            // The else branch (or nothing), then the rest of this path.
            let mut else_stack = std::mem::take(stack);
            if let Some((_, other)) = &ifx.else_branch {
                else_stack.push(Item::Restore(env.clone()));
                match &**other {
                    Expr::Block(b) => push_block(&mut else_stack, &b.block),
                    rest => else_stack.push(Item::Expr(rest)),
                }
            }
            walk(tr, else_stack, else_pc, env.clone(), out)?;
            Ok(true)
        }
        Expr::Block(b) => {
            // A nested block: its bindings go out of scope when it ends.
            stack.push(Item::Restore(env.clone()));
            push_block(stack, &b.block);
            Ok(false)
        }
        _ if semi => Err("only `let`, `if`, `return` and blocks are supported in the body".into()),
        _ if is_last => {
            tr.env = env.clone();
            let v = tr.expr(e)?;
            record(out, pc, v)?;
            Ok(true)
        }
        _ => Err("only `let`, `if`, `return` and blocks are supported in the body".into()),
    }
}

fn record(out: &mut Vec<(Formula, Val)>, pc: Formula, v: Val) -> Result<(), Failure> {
    if out.len() >= MAX_RETURNS {
        return Err("too many return paths to prove".into());
    }
    out.push((pc, v));
    Ok(())
}

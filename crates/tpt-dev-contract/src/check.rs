//! Compile-time proof of `ensures` clauses, and generation of the output function.

use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Block, Expr, FnArg, ItemFn, Pat, Stmt, Type, parse_quote};
use tpt_dev_solver::{Linear, Rational, Solver, Status};

use crate::translate::{Translator, Val};

/// What the solver established about one `ensures` clause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Proven for every integer input that meets the `requires` clauses. No run-time check is needed.
    Proven,
    /// Refuted, with a counterexample message. The run-time check is kept, and a compile error is raised.
    Violated(String),
    /// Not decided at compile time. A run-time check is kept.
    Runtime,
}

/// The integer types. Floats are excluded, since real arithmetic does not model rounding.
fn is_integer_type(ty: &Type) -> bool {
    let Type::Path(tp) = ty else {
        return false;
    };
    let Some(seg) = tp.path.segments.last() else {
        return false;
    };
    matches!(
        seg.ident.to_string().as_str(),
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
    )
}

/// The name bound by a `let` pattern: `x` or `x: T`.
fn binding_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(p) => Some(p.ident.to_string()),
        Pat::Type(pt) => binding_name(&pt.pat),
        _ => None,
    }
}

/// Symbolically runs straight-line `let` statements, then returns the tail value.
fn body_value(tr: &mut Translator<'_>, block: &Block) -> Result<Val, String> {
    let Some((last, init)) = block.stmts.split_last() else {
        return Err("empty body".into());
    };
    for stmt in init {
        match stmt {
            Stmt::Local(local) => {
                let name = binding_name(&local.pat).ok_or("unsupported `let` pattern")?;
                let init = local.init.as_ref().ok_or("`let` without a value")?;
                if init.diverge.is_some() {
                    return Err("`let else` is not supported".into());
                }
                let v = tr.expr(&init.expr)?;
                tr.env.insert(name, v);
            }
            _ => return Err("only `let` statements are supported in the body".into()),
        }
    }
    match last {
        Stmt::Expr(e, None) => tr.expr(e),
        _ => Err("the body must end in an expression".into()),
    }
}

/// Decides each `ensures` clause against the `requires` clauses and the body.
fn analyse(f: &ItemFn, requires: &[Expr], ensures: &[Expr]) -> Vec<Outcome> {
    let runtime = vec![Outcome::Runtime; ensures.len()];
    let mut solver = Solver::new();
    let mut params: HashMap<String, Val> = HashMap::new();
    let mut vars = Vec::new();
    for input in &f.sig.inputs {
        let FnArg::Typed(pt) = input else {
            return runtime;
        };
        let (Pat::Ident(pi), true) = (&*pt.pat, is_integer_type(&pt.ty)) else {
            return runtime;
        };
        let name = pi.ident.to_string();
        let v = solver.declare(&name);
        params.insert(name.clone(), Val::Num(Linear::var(v)));
        vars.push((name, v));
    }

    // Build all formulas in one scope, so the arena borrow ends before solving.
    let (pre, negs) = {
        let mut pres = Vec::new();
        {
            let mut tr = Translator {
                arena: solver.arena_mut(),
                env: params.clone(),
            };
            for r in requires {
                if let Ok(Val::Bool(f)) = tr.expr(r) {
                    pres.push(f);
                }
            }
        }
        let pre = solver.arena_mut().and(pres);

        let mut tr = Translator {
            arena: solver.arena_mut(),
            env: params.clone(),
        };
        let result = body_value(&mut tr, &f.block);
        let mut negs = Vec::with_capacity(ensures.len());
        match result {
            Ok(rv) => {
                for e in ensures {
                    let mut env = params.clone();
                    env.insert("result".into(), rv.clone());
                    let mut et = Translator {
                        arena: &mut *tr.arena,
                        env,
                    };
                    negs.push(match et.expr(e) {
                        Ok(Val::Bool(g)) => Some(et.arena.not(g)),
                        _ => None,
                    });
                }
            }
            Err(_) => negs = vec![None; ensures.len()],
        }
        (pre, negs)
    };

    // The requires clauses must be satisfiable, or every proof would be vacuous.
    solver.push();
    solver.assert(pre);
    let satisfiable = !matches!(solver.check(), Ok(Status::Unsat));
    solver.pop();
    if !satisfiable {
        return runtime;
    }

    negs.iter()
        .map(|neg| {
            let Some(neg) = *neg else {
                return Outcome::Runtime;
            };
            solver.push();
            solver.assert(pre);
            solver.assert(neg);
            let outcome = match solver.check() {
                Ok(Status::Unsat) => Outcome::Proven,
                Ok(Status::Sat) => {
                    let values: Vec<(String, Rational)> = vars
                        .iter()
                        .map(|(n, v)| (n.clone(), solver.value(*v).unwrap_or(Rational::ZERO)))
                        .collect();
                    // Over the reals a refutation may be fractional. Report it only when
                    // the inputs are integers, so the counterexample is real.
                    if values.iter().all(|(_, r)| r.is_integer()) {
                        Outcome::Violated(describe(&values))
                    } else {
                        Outcome::Runtime
                    }
                }
                Err(_) => Outcome::Runtime,
            };
            solver.pop();
            outcome
        })
        .collect()
}

fn describe(values: &[(String, Rational)]) -> String {
    let assignments: Vec<String> = values
        .iter()
        .map(|(n, r)| format!("{n} = {}", r.num()))
        .collect();
    format!("counterexample: {}", assignments.join(", "))
}

/// Produces the function with its contracts checked, and any compile errors.
pub(crate) fn generate(mut f: ItemFn, requires: &[Expr], ensures: &[Expr]) -> TokenStream {
    let outcomes = analyse(&f, requires, ensures);

    let mut errors = TokenStream::new();
    for (e, o) in ensures.iter().zip(&outcomes) {
        if let Outcome::Violated(msg) = o {
            let text = quote!(#e).to_string();
            let err = syn::Error::new(
                e.span(),
                format!("`ensures` clause `{text}` is false: {msg}"),
            );
            errors.extend(err.to_compile_error());
        }
    }

    // `requires` is always checked on entry. Callers must satisfy it.
    let req_checks: Vec<TokenStream> = requires
        .iter()
        .map(|e| {
            let text = quote!(#e).to_string();
            quote! { ::core::debug_assert!(#e, "`requires` clause violated: {}", #text); }
        })
        .collect();
    // `ensures` is checked on return, unless it was proven.
    let ens_checks: Vec<TokenStream> = ensures
        .iter()
        .zip(&outcomes)
        .filter(|(_, o)| **o != Outcome::Proven)
        .map(|(e, _)| {
            let text = quote!(#e).to_string();
            quote! { ::core::debug_assert!(#e, "`ensures` clause violated: {}", #text); }
        })
        .collect();

    let body = &f.block;
    let new_block: Block = if ens_checks.is_empty() {
        parse_quote!({ #(#req_checks)* #body })
    } else {
        parse_quote!({
            #(#req_checks)*
            let result = #body;
            #(#ens_checks)*
            result
        })
    };
    *f.block = new_block;
    quote! { #errors #f }
}

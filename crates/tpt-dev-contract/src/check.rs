//! Compile-time proof of `ensures` clauses, and generation of the output function.

use std::collections::HashMap;

use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use quote::{format_ident, quote};
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
    // Each parameter also gets an entry-snapshot variable, which `old(x)` denotes.
    let mut snapshots: HashMap<String, Val> = HashMap::new();
    let mut snapshot_eqs = Vec::new();
    for input in &f.sig.inputs {
        let FnArg::Typed(pt) = input else {
            return runtime;
        };
        let (Pat::Ident(pi), true) = (&*pt.pat, is_integer_type(&pt.ty)) else {
            return runtime;
        };
        let name = pi.ident.to_string();
        let v = solver.declare(&name);
        let snap = solver.declare(&format!("old({name})"));
        params.insert(name.clone(), Val::Num(Linear::var(v)));
        snapshots.insert(name.clone(), Val::Num(Linear::var(snap)));
        snapshot_eqs.push((Linear::var(snap), Linear::var(v)));
        vars.push((name, v));
    }

    // Build all formulas in one scope, so the arena borrow ends before solving.
    let (pre, negs) = {
        let mut pres = Vec::new();
        {
            let mut tr = Translator {
                arena: solver.arena_mut(),
                env: params.clone(),
                old: HashMap::new(),
            };
            for r in requires {
                if let Ok(Val::Bool(f)) = tr.expr(r) {
                    pres.push(f);
                }
            }
            // A snapshot holds the entry value of its parameter.
            for (snap, v) in &snapshot_eqs {
                if let Some(eq) = tr.arena.eq(snap, v) {
                    pres.push(eq);
                }
            }
        }
        let pre = solver.arena_mut().and(pres);

        let mut tr = Translator {
            arena: solver.arena_mut(),
            env: params.clone(),
            old: HashMap::new(),
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
                        old: snapshots.clone(),
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

/// Prefix of the run-time variable that holds the entry value of a parameter.
const SNAPSHOT_PREFIX: &str = "__tpt_old_";

/// The run-time variable that holds the entry value of parameter `name`.
fn snapshot_name(name: &str) -> String {
    format!("{SNAPSHOT_PREFIX}{name}")
}

/// The single identifier inside a parenthesised group, as in `old(x)`.
fn single_name(g: &Group) -> Option<String> {
    if g.delimiter() != Delimiter::Parenthesis {
        return None;
    }
    let mut it = g.stream().into_iter();
    match (it.next(), it.next()) {
        (Some(TokenTree::Ident(i)), None) => Some(i.to_string()),
        _ => None,
    }
}

/// Collects the parameter names used in `old(x)` calls. Returns the span of an
/// `old` whose argument is not a single name.
fn collect_old(ts: TokenStream, names: &mut Vec<String>) -> Result<(), Span> {
    let mut iter = ts.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match &tt {
            TokenTree::Ident(id) if id == "old" => {
                if let Some(TokenTree::Group(g)) = iter.peek()
                    && g.delimiter() == Delimiter::Parenthesis
                {
                    match single_name(g) {
                        Some(name) => {
                            names.push(name);
                            iter.next();
                        }
                        None => return Err(id.span()),
                    }
                }
            }
            TokenTree::Group(g) => collect_old(g.stream(), names)?,
            _ => {}
        }
    }
    Ok(())
}

/// Replaces each `old(x)` with the run-time snapshot variable of `x`.
fn rewrite_old(ts: TokenStream) -> TokenStream {
    let mut out: Vec<TokenTree> = Vec::new();
    let mut iter = ts.into_iter().peekable();
    while let Some(tt) = iter.next() {
        if let TokenTree::Ident(id) = &tt
            && id == "old"
            && let Some(TokenTree::Group(g)) = iter.peek()
            && let Some(name) = single_name(g)
        {
            out.push(TokenTree::Ident(Ident::new(
                &snapshot_name(&name),
                id.span(),
            )));
            iter.next();
            continue;
        }
        match tt {
            TokenTree::Group(g) => {
                let mut rewritten = Group::new(g.delimiter(), rewrite_old(g.stream()));
                rewritten.set_span(g.span());
                out.push(TokenTree::Group(rewritten));
            }
            other => out.push(other),
        }
    }
    out.into_iter().collect()
}

/// Names of the function's parameters that are plain identifiers.
fn param_names(f: &ItemFn) -> Vec<String> {
    f.sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pt) => match &*pt.pat {
                Pat::Ident(pi) => Some(pi.ident.to_string()),
                _ => None,
            },
            FnArg::Receiver(_) => None,
        })
        .collect()
}

/// Produces the function with its contracts checked, and any compile errors.
pub(crate) fn generate(mut f: ItemFn, requires: &[Expr], ensures: &[Expr]) -> TokenStream {
    let outcomes = analyse(&f, requires, ensures);
    let params = param_names(&f);

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

    // `old` is only meaningful in `ensures`, where it names the entry value of a parameter.
    for r in requires {
        let mut names = Vec::new();
        if collect_old(quote!(#r), &mut names).is_err() || !names.is_empty() {
            let err = syn::Error::new(r.span(), "`old()` is only allowed in `ensures`");
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

    // `ensures` is checked on return, unless it was proven. Each parameter used
    // under `old` is copied on entry, before the body can change it.
    let mut snapshots: Vec<String> = Vec::new();
    let mut ens_checks = Vec::new();
    for (e, o) in ensures.iter().zip(&outcomes) {
        if *o == Outcome::Proven {
            continue;
        }
        let mut names = Vec::new();
        if collect_old(quote!(#e), &mut names).is_err() {
            let err = syn::Error::new(e.span(), "`old()` takes one parameter name");
            errors.extend(err.to_compile_error());
        }
        for n in names {
            if !params.contains(&n) {
                let msg = format!("`old({n})`: `{n}` is not a parameter of this function");
                errors.extend(syn::Error::new(e.span(), msg).to_compile_error());
            } else if !snapshots.contains(&n) {
                snapshots.push(n);
            }
        }
        let text = quote!(#e).to_string();
        let checked: Expr = syn::parse2(rewrite_old(quote!(#e))).unwrap_or_else(|_| e.clone());
        ens_checks.push(
            quote! { ::core::debug_assert!(#checked, "`ensures` clause violated: {}", #text); },
        );
    }
    let snap_lets: Vec<TokenStream> = snapshots
        .iter()
        .map(|n| {
            let param = format_ident!("{}", n);
            let snap = format_ident!("{}", snapshot_name(n));
            quote! { let #snap = #param; }
        })
        .collect();

    let body = &f.block;
    let new_block: Block = if ens_checks.is_empty() {
        parse_quote!({ #(#req_checks)* #body })
    } else {
        parse_quote!({
            #(#req_checks)*
            #(#snap_lets)*
            let result = #body;
            #(#ens_checks)*
            result
        })
    };
    *f.block = new_block;
    quote! { #errors #f }
}

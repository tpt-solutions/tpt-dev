//! The `#[circuit]` attribute for `tpt-dev-zk`.
//!
//! Applied to a function over `i64`, it keeps the function and writes a second
//! one, `<name>_circuit`, generic over `Ring`. The second function has the same
//! body, with each constant made in the circuit and each name cloned, so calling
//! it with wires records the circuit. The supported syntax is integer literals,
//! parameters, `let` bindings, parentheses, `+`, `-`, `*`, and unary `-`. Anything
//! else is a compile error.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    BinOp, Block, Expr, FnArg, Ident, ItemFn, Lit, Pat, ReturnType, Stmt, Type, UnOp,
    parse_macro_input,
};

/// Writes a circuit version of an `i64` function. See the crate documentation.
///
/// # Examples
///
/// This example is not compiled here: this crate cannot depend on `tpt-dev-zk`
/// without a dependency cycle. The same behaviour is exercised by
/// `tpt-dev-zk`'s tests.
///
/// ```ignore
/// use tpt_dev_zk::{Circuit, circuit};
///
/// #[circuit]
/// fn square_plus(x: i64, k: i64) -> i64 {
///     x * x + k
/// }
///
/// let c = Circuit::new();
/// let x = c.private_input(4);
/// let k = c.public_input(1);
/// let out = c.output(&square_plus_circuit(x, k));
/// assert!(c.check().is_ok());
/// assert_eq!(c.value(out), square_plus(4, 1));
/// ```
#[proc_macro_attribute]
pub fn circuit(attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    if !attr.is_empty() {
        let err = syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[circuit] takes no arguments",
        )
        .to_compile_error();
        return quote!(#func #err).into();
    }
    match expand(&func) {
        Ok(generated) => quote!(#func #generated).into(),
        Err(err) => {
            let err = err.to_compile_error();
            quote!(#func #err).into()
        }
    }
}

fn is_i64(ty: &Type) -> bool {
    match ty {
        Type::Path(p) => p.qself.is_none() && p.path.is_ident("i64"),
        _ => false,
    }
}

fn expand(func: &ItemFn) -> syn::Result<TokenStream2> {
    if !func.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &func.sig.generics,
            "#[circuit] does not support generic functions",
        ));
    }
    let mut params: Vec<Ident> = Vec::new();
    for arg in &func.sig.inputs {
        let FnArg::Typed(pat_type) = arg else {
            return Err(syn::Error::new_spanned(
                arg,
                "#[circuit] does not support methods",
            ));
        };
        let Pat::Ident(pat) = &*pat_type.pat else {
            return Err(syn::Error::new_spanned(
                &pat_type.pat,
                "#[circuit] parameters must be plain names",
            ));
        };
        if !is_i64(&pat_type.ty) {
            return Err(syn::Error::new_spanned(
                &pat_type.ty,
                "#[circuit] supports only i64 parameters",
            ));
        }
        params.push(pat.ident.clone());
    }
    match &func.sig.output {
        ReturnType::Type(_, ty) if is_i64(ty) => {}
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "#[circuit] functions must return i64",
            ));
        }
    }
    let Some(first) = params.first().cloned() else {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[circuit] needs at least one parameter, to make constants from",
        ));
    };
    let ctx = format_ident!("__zk_ctx");
    let mut env = params.clone();
    let body = translate_block(&func.block, &mut env, &ctx)?;

    let vis = &func.vis;
    let name = format_ident!("{}_circuit", func.sig.ident);
    Ok(quote! {
        #[allow(unused_variables, non_snake_case)]
        #vis fn #name<__T: ::tpt_dev_zk::Ring>(#(#params: __T),*) -> __T {
            let #ctx = #first.clone();
            #body
        }
    })
}

/// Translates a block. Every statement must be a `let` with a plain name, or the final expression.
fn translate_block(block: &Block, env: &mut Vec<Ident>, ctx: &Ident) -> syn::Result<TokenStream2> {
    let mut out = TokenStream2::new();
    let count = block.stmts.len();
    for (i, stmt) in block.stmts.iter().enumerate() {
        match stmt {
            Stmt::Local(local) => {
                let Pat::Ident(pat) = &local.pat else {
                    return Err(syn::Error::new_spanned(
                        &local.pat,
                        "#[circuit] let bindings must be plain names",
                    ));
                };
                let Some(init) = &local.init else {
                    return Err(syn::Error::new_spanned(
                        local,
                        "#[circuit] let bindings need a value",
                    ));
                };
                if local.init.as_ref().is_some_and(|i| i.diverge.is_some()) {
                    return Err(syn::Error::new_spanned(
                        local,
                        "#[circuit] does not support let-else",
                    ));
                }
                let value = translate_expr(&init.expr, env, ctx)?;
                let name = &pat.ident;
                out.extend(quote!(let #name = #value;));
                env.push(name.clone());
            }
            Stmt::Expr(expr, None) if i + 1 == count => {
                out.extend(translate_expr(expr, env, ctx)?);
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "#[circuit] supports let bindings and a final expression only",
                ));
            }
        }
    }
    if count == 0 {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[circuit] function body is empty",
        ));
    }
    Ok(out)
}

fn translate_expr(expr: &Expr, env: &mut Vec<Ident>, ctx: &Ident) -> syn::Result<TokenStream2> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(int) => {
                let value: i64 = int.base10_parse()?;
                Ok(quote!(::tpt_dev_zk::Ring::lit(&#ctx, #value)))
            }
            other => Err(syn::Error::new_spanned(
                other,
                "#[circuit] supports integer literals only",
            )),
        },
        Expr::Path(path) if path.qself.is_none() && path.path.get_ident().is_some() => {
            let ident = path
                .path
                .get_ident()
                .cloned()
                .unwrap_or_else(|| unreachable!("guarded by get_ident above"));
            if env.contains(&ident) {
                Ok(quote!(#ident.clone()))
            } else {
                Err(syn::Error::new_spanned(
                    &ident,
                    "#[circuit] unknown name: only parameters and earlier let bindings",
                ))
            }
        }
        Expr::Paren(paren) => {
            let inner = translate_expr(&paren.expr, env, ctx)?;
            Ok(quote!((#inner)))
        }
        Expr::Binary(binary) => {
            let op = match binary.op {
                BinOp::Add(_) => quote!(+),
                BinOp::Sub(_) => quote!(-),
                BinOp::Mul(_) => quote!(*),
                _ => {
                    return Err(syn::Error::new_spanned(
                        binary.op,
                        "#[circuit] supports +, - and * only",
                    ));
                }
            };
            let left = translate_expr(&binary.left, env, ctx)?;
            let right = translate_expr(&binary.right, env, ctx)?;
            Ok(quote!((#left) #op (#right)))
        }
        Expr::Unary(unary) => match unary.op {
            UnOp::Neg(_) => {
                let inner = translate_expr(&unary.expr, env, ctx)?;
                Ok(quote!((::tpt_dev_zk::Ring::lit(&#ctx, 0) - (#inner))))
            }
            _ => Err(syn::Error::new_spanned(
                unary.op,
                "#[circuit] supports unary - only",
            )),
        },
        Expr::Block(block) if block.label.is_none() => {
            let inner = translate_block(&block.block, env, ctx)?;
            Ok(quote!({ #inner }))
        }
        other => Err(syn::Error::new_spanned(
            other,
            "#[circuit] unsupported expression: use integer literals, names, parentheses, + - * and unary -",
        )),
    }
}

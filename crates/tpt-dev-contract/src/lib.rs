//! `#[requires]` and `#[ensures]` attribute macros.
//!
//! A `requires` clause is checked at run time, in debug builds, when the
//! function is entered. An `ensures` clause is checked when the function
//! returns. When the body is straight-line linear code, the solver tries to
//! prove each `ensures` clause at compile time. A proven clause adds no
//! run-time check. A clause the solver refutes is a compile error, with a
//! counterexample.

use proc_macro::TokenStream;
use syn::{Expr, ItemFn, parse_macro_input};

mod body;
mod check;
mod translate;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Requires,
    Ensures,
}

/// Precondition of a function: `#[requires(x >= 0)]`.
///
/// Also accepts any `#[ensures]` attributes on the same function.
///
/// # Examples
///
/// ```
/// use tpt_dev_contract::requires;
///
/// #[requires(x >= 0)]
/// fn sqrt_floor(x: i64) -> i64 {
///     let mut r = 0;
///     while (r + 1) * (r + 1) <= x {
///         r += 1;
///     }
///     r
/// }
///
/// assert_eq!(sqrt_floor(10), 3);
/// ```
#[proc_macro_attribute]
pub fn requires(args: TokenStream, item: TokenStream) -> TokenStream {
    contract(Kind::Requires, args, item)
}

/// Postcondition of a function: `#[ensures(result >= x)]`. The name `result`
/// is the return value.
///
/// Also accepts any `#[requires]` attributes on the same function.
///
/// # Examples
///
/// ```
/// use tpt_dev_contract::ensures;
///
/// #[ensures(result >= x)]
/// fn bump(x: i64) -> i64 {
///     x + 1
/// }
///
/// assert_eq!(bump(4), 5);
/// ```
#[proc_macro_attribute]
pub fn ensures(args: TokenStream, item: TokenStream) -> TokenStream {
    contract(Kind::Ensures, args, item)
}

/// Matches `#[requires]` and `#[tpt_dev_contract::requires]` alike.
fn last_segment_is(attr: &syn::Attribute, name: &str) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|seg| seg.ident == name)
}

fn contract(kind: Kind, args: TokenStream, item: TokenStream) -> TokenStream {
    let mut f = parse_macro_input!(item as ItemFn);
    let first = match syn::parse::<Expr>(args) {
        Ok(e) => e,
        Err(e) => return e.to_compile_error().into(),
    };
    let mut requires = Vec::new();
    let mut ensures = Vec::new();
    match kind {
        Kind::Requires => requires.push(first),
        Kind::Ensures => ensures.push(first),
    }
    // Gather the other contract attributes on the item, and keep the rest.
    let mut kept = Vec::new();
    for attr in std::mem::take(&mut f.attrs) {
        if last_segment_is(&attr, "requires") {
            match attr.parse_args::<Expr>() {
                Ok(e) => requires.push(e),
                Err(e) => return e.to_compile_error().into(),
            }
        } else if last_segment_is(&attr, "ensures") {
            match attr.parse_args::<Expr>() {
                Ok(e) => ensures.push(e),
                Err(e) => return e.to_compile_error().into(),
            }
        } else {
            kept.push(attr);
        }
    }
    f.attrs = kept;
    check::generate(f, &requires, &ensures).into()
}

//! The `dsl!` macro. It parses a grammar written in a small PEG notation at
//! compile time, and expands to a `tpt_dev_dsl::Grammar` constant.
//!
//! Use it through `tpt_dev_dsl::dsl!`, with the `macros` feature of
//! `tpt-dev-dsl` enabled.

use proc_macro::TokenStream;
use proc_macro2::{Literal, Span, TokenStream as TokenStream2};
use quote::quote;
use std::collections::HashMap;
use syn::parse::{Parse, ParseStream};
use syn::{
    Ident, LitByte, LitByteStr, LitChar, LitStr, Token, Visibility, braced, bracketed,
    parenthesized,
};

// The validation of a grammar is the one in tpt-dev-dsl, compiled into this
// crate from the same file. This crate cannot depend on tpt-dev-dsl, because
// tpt-dev-dsl depends on it through its `macros` feature.
#[allow(dead_code, unreachable_pub)]
#[path = "../../tpt-dev-dsl/src/grammar.rs"]
mod grammar;

/// Parses a grammar and expands to a `const` of type `tpt_dev_dsl::Grammar`.
///
/// ```text
/// dsl! {
///     pub ARITH {
///         expr = term (space ("+" / "-") space term)*;
///         term = factor (space ("*" / "/") space factor)*;
///         factor = number / "(" space expr space ")";
///         number = ['0'-'9'] ['0'-'9']*;
///         space = " "*;
///     }
/// }
/// ```
///
/// The notation:
///
/// - `"text"` matches those bytes. `""` matches nothing.
/// - `['a'-'z']` matches one ASCII byte in the range.
/// - `name` calls a rule. It must be defined in the same block.
/// - `e1 e2` matches a sequence. `e1 / e2` tries the alternatives in order.
/// - `e*` is zero or more, and `e?` is optional. `+` is not supported: write `e e*`.
/// - `( e )` groups.
///
/// Rules are numbered in the order they are written, and the first rule is the
/// start rule. Errors point at the offending token.
#[proc_macro]
pub fn dsl(input: TokenStream) -> TokenStream {
    expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// A parsed grammar block.
struct GrammarDef {
    vis: Visibility,
    name: Ident,
    rules: Vec<RuleDef>,
}

/// One rule: `name = body;`.
struct RuleDef {
    name: Ident,
    body: Node,
}

/// A parsed expression, before rule names are resolved.
enum Node {
    Lit(LitStr),
    Range(LitChar, LitChar),
    Call(Ident),
    Seq(Vec<Node>),
    Choice(Vec<Node>),
    Repeat(Box<Node>),
    Optional(Box<Node>),
}

impl Parse for GrammarDef {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let vis: Visibility = input.parse()?;
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let mut rules = Vec::new();
        while !content.is_empty() {
            rules.push(content.parse::<RuleDef>()?);
        }
        Ok(Self { vis, name, rules })
    }
}

impl Parse for RuleDef {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let body = parse_choice(input)?;
        input.parse::<Token![;]>()?;
        Ok(Self { name, body })
    }
}

fn parse_choice(input: ParseStream) -> syn::Result<Node> {
    let mut alts = vec![parse_seq(input)?];
    while input.peek(Token![/]) {
        input.parse::<Token![/]>()?;
        alts.push(parse_seq(input)?);
    }
    if alts.len() == 1 {
        Ok(alts.remove(0))
    } else {
        Ok(Node::Choice(alts))
    }
}

fn starts_primary(input: ParseStream) -> bool {
    input.peek(LitStr)
        || input.peek(Ident)
        || input.peek(syn::token::Bracket)
        || input.peek(syn::token::Paren)
}

fn parse_seq(input: ParseStream) -> syn::Result<Node> {
    let mut items = Vec::new();
    while starts_primary(input) {
        items.push(parse_postfix(input)?);
    }
    match items.len() {
        0 => Err(input.error("expected a string, a rule name, a [range] or a (group)")),
        1 => Ok(items.remove(0)),
        _ => Ok(Node::Seq(items)),
    }
}

fn parse_postfix(input: ParseStream) -> syn::Result<Node> {
    let mut node = parse_primary(input)?;
    loop {
        if input.peek(Token![*]) {
            input.parse::<Token![*]>()?;
            node = Node::Repeat(Box::new(node));
        } else if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            node = Node::Optional(Box::new(node));
        } else if input.peek(Token![+]) {
            return Err(input.error("`+` is not supported; write `e e*` for one or more"));
        } else {
            return Ok(node);
        }
    }
}

fn parse_primary(input: ParseStream) -> syn::Result<Node> {
    if input.peek(LitStr) {
        Ok(Node::Lit(input.parse()?))
    } else if input.peek(Ident) {
        Ok(Node::Call(input.parse()?))
    } else if input.peek(syn::token::Bracket) {
        let content;
        bracketed!(content in input);
        let lo: LitChar = content.parse()?;
        content.parse::<Token![-]>()?;
        let hi: LitChar = content.parse()?;
        if !content.is_empty() {
            return Err(content.error("expected `]` after the range"));
        }
        Ok(Node::Range(lo, hi))
    } else if input.peek(syn::token::Paren) {
        let content;
        parenthesized!(content in input);
        let inner = parse_choice(&content)?;
        if !content.is_empty() {
            return Err(content.error("expected `)` to close the group"));
        }
        Ok(inner)
    } else {
        Err(input.error("expected a string, a rule name, a [range] or a (group)"))
    }
}

/// The byte range `['lo'-'hi']`, checked to be ASCII and in order.
fn byte_range(lo: &LitChar, hi: &LitChar) -> syn::Result<(u8, u8)> {
    let (l, h) = (lo.value(), hi.value());
    if !l.is_ascii() || !h.is_ascii() {
        return Err(syn::Error::new(lo.span(), "range bounds must be ASCII"));
    }
    if l > h {
        return Err(syn::Error::new(
            lo.span(),
            "range is empty: the start is after the end",
        ));
    }
    Ok((l as u8, h as u8))
}

fn leak<T>(v: Vec<T>) -> &'static [T] {
    Box::leak(v.into_boxed_slice())
}

/// Resolves rule names and builds the expression that `grammar::validate` checks.
fn lower(node: &Node, index: &HashMap<String, usize>) -> syn::Result<grammar::Expr> {
    use grammar::Expr;
    Ok(match node {
        Node::Lit(s) => Expr::Lit(leak(s.value().into_bytes())),
        Node::Range(lo, hi) => {
            let (lo, hi) = byte_range(lo, hi)?;
            Expr::Range(lo, hi)
        }
        Node::Call(id) => match index.get(&id.to_string()) {
            Some(&i) => Expr::Call(i),
            None => {
                return Err(syn::Error::new(
                    id.span(),
                    format!("rule `{id}` is not defined in this grammar"),
                ));
            }
        },
        Node::Seq(items) => Expr::Seq(leak(
            items
                .iter()
                .map(|n| lower(n, index))
                .collect::<syn::Result<Vec<_>>>()?,
        )),
        Node::Choice(alts) => Expr::Choice(leak(
            alts.iter()
                .map(|n| lower(n, index))
                .collect::<syn::Result<Vec<_>>>()?,
        )),
        Node::Repeat(inner) => Expr::Repeat(Box::leak(Box::new(lower(inner, index)?))),
        Node::Optional(inner) => Expr::Optional(Box::leak(Box::new(lower(inner, index)?))),
    })
}

/// The tokens for a resolved expression. Only called after `lower` succeeded.
fn emit(node: &Node, index: &HashMap<String, usize>) -> syn::Result<TokenStream2> {
    Ok(match node {
        Node::Lit(s) => {
            let bytes = LitByteStr::new(&s.value().into_bytes(), s.span());
            quote!(::tpt_dev_dsl::Expr::Lit(#bytes))
        }
        Node::Range(lo, hi) => {
            let (lo, hi) = byte_range(lo, hi)?;
            let lo = LitByte::new(lo, Span::call_site());
            let hi = LitByte::new(hi, Span::call_site());
            quote!(::tpt_dev_dsl::Expr::Range(#lo, #hi))
        }
        Node::Call(id) => {
            let i = Literal::usize_unsuffixed(index.get(&id.to_string()).copied().unwrap_or(0));
            quote!(::tpt_dev_dsl::Expr::Call(#i))
        }
        Node::Seq(items) => {
            let items = items
                .iter()
                .map(|n| emit(n, index))
                .collect::<syn::Result<Vec<_>>>()?;
            quote!(::tpt_dev_dsl::Expr::Seq(&[#(#items),*]))
        }
        Node::Choice(alts) => {
            let alts = alts
                .iter()
                .map(|n| emit(n, index))
                .collect::<syn::Result<Vec<_>>>()?;
            quote!(::tpt_dev_dsl::Expr::Choice(&[#(#alts),*]))
        }
        Node::Repeat(inner) => {
            let inner = emit(inner, index)?;
            quote!(::tpt_dev_dsl::Expr::Repeat(&#inner))
        }
        Node::Optional(inner) => {
            let inner = emit(inner, index)?;
            quote!(::tpt_dev_dsl::Expr::Optional(&#inner))
        }
    })
}

fn expand(input: TokenStream2) -> syn::Result<TokenStream2> {
    let def: GrammarDef = syn::parse2(input)?;

    let mut index = HashMap::new();
    for (i, rule) in def.rules.iter().enumerate() {
        if index.insert(rule.name.to_string(), i).is_some() {
            return Err(syn::Error::new(
                rule.name.span(),
                format!("rule `{}` is defined more than once", rule.name),
            ));
        }
    }

    // Check the grammar with the same validation that `Grammar::new` runs.
    let mut lowered = Vec::with_capacity(def.rules.len());
    for rule in &def.rules {
        lowered.push(grammar::Rule {
            name: Box::leak(rule.name.to_string().into_boxed_str()),
            body: lower(&rule.body, &index)?,
        });
    }
    if let Err(e) = grammar::validate(leak(lowered)) {
        let message = match e {
            grammar::GrammarError::Empty => "a grammar needs at least one rule".to_owned(),
            grammar::GrammarError::UnknownRule { .. } => {
                "a rule refers to a rule that does not exist".to_owned()
            }
        };
        return Err(syn::Error::new(def.name.span(), message));
    }

    let mut rules = Vec::with_capacity(def.rules.len());
    for rule in &def.rules {
        let name = rule.name.to_string();
        let body = emit(&rule.body, &index)?;
        rules.push(quote!(::tpt_dev_dsl::Rule { name: #name, body: #body }));
    }

    let vis = &def.vis;
    let name = &def.name;
    let doc = format!("The `{name}` grammar, written with `dsl!`.");
    Ok(quote! {
        #[doc = #doc]
        #vis const #name: ::tpt_dev_dsl::Grammar =
            ::tpt_dev_dsl::Grammar::new(&[#(#rules),*]);
    })
}

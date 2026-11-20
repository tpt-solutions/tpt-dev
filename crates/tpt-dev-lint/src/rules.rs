//! The built-in rules. Each is a syntax-tree visitor that skips test code.

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Expr, ExprLit, ExprMethodCall, ExprUnsafe, File, Ident, ImplItem, ItemFn, ItemImpl,
    ItemMod, Lit, Macro, Meta, Visibility,
};

use crate::{Finding, Rule, Severity};

/// Returns `true` for attributes that mark test-only code: `#[test]` and `#[cfg(test)]`.
fn is_test_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("test")
            || (a.path().is_ident("cfg")
                && a.meta
                    .require_list()
                    .is_ok_and(|l| l.tokens.to_string().contains("test")))
    })
}

fn location(span: Span) -> (usize, usize) {
    let start = span.start();
    (start.line, start.column + 1)
}

fn finding(span: Span, message: String) -> Finding {
    let (line, column) = location(span);
    Finding {
        line,
        column,
        message,
    }
}

/// Flags `.unwrap()` and `.expect(..)` in non-test code.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoUnwrap;

struct UnwrapVisitor {
    findings: Vec<Finding>,
}

impl<'ast> Visit<'ast> for UnwrapVisitor {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !is_test_attr(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if !is_test_attr(&f.attrs) {
            visit::visit_item_fn(self, f);
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        let name = call.method.to_string();
        if name == "unwrap" || name == "expect" {
            self.findings.push(finding(
                call.method.span(),
                format!("`.{name}()` can panic; return the error or handle the `None`/`Err` case"),
            ));
        }
        visit::visit_expr_method_call(self, call);
    }
}

impl Rule for NoUnwrap {
    fn name(&self) -> &'static str {
        "no-unwrap"
    }

    fn check(&self, file: &File, _source: &str) -> Vec<Finding> {
        let mut v = UnwrapVisitor {
            findings: Vec::new(),
        };
        v.visit_file(file);
        v.findings
    }
}

/// Flags `todo!()` and `unimplemented!()` in non-test code.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoTodoMacros;

struct TodoVisitor {
    findings: Vec<Finding>,
}

impl<'ast> Visit<'ast> for TodoVisitor {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !is_test_attr(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if !is_test_attr(&f.attrs) {
            visit::visit_item_fn(self, f);
        }
    }

    fn visit_macro(&mut self, m: &'ast Macro) {
        if let Some(seg) = m.path.segments.last() {
            let name = seg.ident.to_string();
            if name == "todo" || name == "unimplemented" {
                self.findings.push(finding(
                    seg.ident.span(),
                    format!("`{name}!` left in code; finish the implementation or return an error"),
                ));
            }
        }
        visit::visit_macro(self, m);
    }
}

impl Rule for NoTodoMacros {
    fn name(&self) -> &'static str {
        "no-todo-macros"
    }

    fn check(&self, file: &File, _source: &str) -> Vec<Finding> {
        let mut v = TodoVisitor {
            findings: Vec::new(),
        };
        v.visit_file(file);
        v.findings
    }
}

/// Requires a `SAFETY` comment within three lines above each `unsafe` block.
#[derive(Clone, Copy, Debug, Default)]
pub struct SafetyComment;

struct SafetyVisitor<'s> {
    lines: Vec<&'s str>,
    findings: Vec<Finding>,
}

impl<'ast> Visit<'ast> for SafetyVisitor<'_> {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !is_test_attr(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if !is_test_attr(&f.attrs) {
            visit::visit_item_fn(self, f);
        }
    }

    fn visit_expr_unsafe(&mut self, u: &'ast ExprUnsafe) {
        let (line, _) = location(u.unsafe_token.span());
        // Lines are 1-based. Check the three lines above the block, and the block's own line.
        let first = line.saturating_sub(3).max(1);
        let documented = (first..=line).any(|l| {
            self.lines
                .get(l - 1)
                .is_some_and(|text| text.contains("SAFETY"))
        });
        if !documented {
            self.findings.push(finding(
                u.unsafe_token.span(),
                "`unsafe` block has no `SAFETY` comment above it explaining why it is sound".into(),
            ));
        }
        visit::visit_expr_unsafe(self, u);
    }
}

impl Rule for SafetyComment {
    fn name(&self) -> &'static str {
        "safety-comment"
    }

    fn check(&self, file: &File, source: &str) -> Vec<Finding> {
        let mut v = SafetyVisitor {
            lines: source.lines().collect(),
            findings: Vec::new(),
        };
        v.visit_file(file);
        v.findings
    }
}

/// Flags a `pub` function, or a `pub` method in an inherent `impl`, whose doc comment has no
/// fenced code block (a line starting with three backticks).
///
/// Restricted visibilities such as `pub(crate)` and `pub(super)` are not checked. Trait impl
/// methods are not checked, because they take their visibility from the trait. Whether the
/// enclosing type is itself public is not analysed.
#[derive(Clone, Copy, Debug, Default)]
pub struct DocExample;

struct DocExampleVisitor {
    findings: Vec<Finding>,
}

impl DocExampleVisitor {
    fn check(&mut self, name: &Ident, attrs: &[Attribute]) {
        if !has_fenced_example(attrs) {
            self.findings.push(finding(
                name.span(),
                format!(
                    "public function `{name}` has no example; add a fenced code block (```) to its doc comment"
                ),
            ));
        }
    }
}

/// The text of a `#[doc = "..."]` attribute, if its value is a string literal.
/// Doc comments (`///`) become these attributes, one per line.
fn doc_text(attr: &Attribute) -> Option<String> {
    if !attr.path().is_ident("doc") {
        return None;
    }
    match &attr.meta {
        Meta::NameValue(nv) => match &nv.value {
            Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            }) => Some(s.value()),
            _ => None,
        },
        _ => None,
    }
}

/// Whether any doc line opens a fenced code block with three backticks.
fn has_fenced_example(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .filter_map(doc_text)
        .any(|text| text.lines().any(|l| l.trim_start().starts_with("```")))
}

fn is_public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

impl<'ast> Visit<'ast> for DocExampleVisitor {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !is_test_attr(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if !is_test_attr(&f.attrs) {
            if is_public(&f.vis) {
                self.check(&f.sig.ident, &f.attrs);
            }
            visit::visit_item_fn(self, f);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        if i.trait_.is_none() {
            for item in &i.items {
                match item {
                    ImplItem::Fn(m) if is_public(&m.vis) && !is_test_attr(&m.attrs) => {
                        self.check(&m.sig.ident, &m.attrs);
                    }
                    _ => {}
                }
            }
        }
        visit::visit_item_impl(self, i);
    }
}

impl Rule for DocExample {
    fn name(&self) -> &'static str {
        "doc-example"
    }

    fn check(&self, file: &File, _source: &str) -> Vec<Finding> {
        let mut v = DocExampleVisitor {
            findings: Vec::new(),
        };
        v.visit_file(file);
        v.findings
    }
}

/// The built-in rules, each at its default severity: the two safety rules deny, and the rest warn.
///
/// # Examples
///
/// ```
/// use tpt_dev_lint::{Rule, Severity, default_rules};
///
/// let rules = default_rules();
/// assert_eq!(rules.len(), 4);
/// assert!(rules.iter().any(|(rule, sev)| rule.name() == "safety-comment" && *sev == Severity::Deny));
/// ```
pub fn default_rules() -> Vec<(Box<dyn Rule>, Severity)> {
    vec![
        (Box::new(NoUnwrap), Severity::Warn),
        (Box::new(NoTodoMacros), Severity::Warn),
        (Box::new(SafetyComment), Severity::Deny),
        (Box::new(DocExample), Severity::Warn),
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn run(rule: &dyn Rule, src: &str) -> Vec<Finding> {
        let file = syn::parse_file(src).unwrap();
        rule.check(&file, src)
    }

    #[test]
    fn unwrap_in_code_is_flagged_but_not_in_tests() {
        let src =
            "fn f(x: Option<u8>) -> u8 { x.unwrap() }\n#[test]\nfn t() { None::<u8>.unwrap(); }\n";
        let found = run(&NoUnwrap, src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
    }

    #[test]
    fn todo_macros_are_flagged() {
        let found = run(
            &NoTodoMacros,
            "fn f() { todo!() }\nfn g() { unimplemented!(\"later\") }\n",
        );
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn unsafe_needs_a_safety_comment_above() {
        let good = "fn f(p: *const u8) -> u8 {\n    // SAFETY: p is valid for reads here.\n    unsafe { *p }\n}\n";
        let bad = "fn f(p: *const u8) -> u8 {\n    unsafe { *p }\n}\n";
        assert!(run(&SafetyComment, good).is_empty());
        assert_eq!(run(&SafetyComment, bad).len(), 1);
    }

    #[test]
    fn documented_pub_fn_with_example_passes() {
        let src = "/// Adds one.\n///\n/// ```\n/// assert_eq!(add_one(1), 2);\n/// ```\npub fn add_one(x: u8) -> u8 { x + 1 }\n";
        assert!(run(&DocExample, src).is_empty());
    }

    #[test]
    fn undocumented_pub_fn_is_flagged_with_location() {
        let src = "/// Adds one, with no example.\npub fn add_one(x: u8) -> u8 { x + 1 }\n";
        let found = run(&DocExample, src);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].line, found[0].column), (2, 8));
    }

    #[test]
    fn pub_crate_and_private_fns_are_ignored() {
        let src = "pub(crate) fn a() {}\npub(super) fn b() {}\nfn c() {}\n";
        assert!(run(&DocExample, src).is_empty());
    }

    #[test]
    fn inherent_pub_methods_checked_and_trait_methods_not() {
        let src = "pub struct S;\nimpl S {\n    pub fn bare(&self) {}\n}\nimpl std::fmt::Debug for S {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { Ok(()) }\n}\n";
        let found = run(&DocExample, src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
    }

    #[test]
    fn test_code_is_skipped_for_doc_example() {
        let src = "#[cfg(test)]\nmod tests {\n    pub fn helper() {}\n}\n";
        assert!(run(&DocExample, src).is_empty());
    }

    #[test]
    fn test_modules_are_skipped() {
        let src = "#[cfg(test)]\nmod tests {\n    fn t() { None::<u8>.unwrap(); }\n}\n";
        assert!(run(&NoUnwrap, src).is_empty());
    }
}

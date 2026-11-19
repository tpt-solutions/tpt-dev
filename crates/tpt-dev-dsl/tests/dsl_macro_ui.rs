//! Compile-time behaviour of `dsl!`: valid grammars expand, and bad grammars are
//! rejected with an error at the offending token.
#![cfg(feature = "macros")]
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/valid.rs");
    t.compile_fail("tests/ui/undefined_rule.rs");
    t.compile_fail("tests/ui/bad_syntax.rs");
    t.compile_fail("tests/ui/plus_unsupported.rs");
    t.compile_fail("tests/ui/duplicate_rule.rs");
    t.compile_fail("tests/ui/empty_grammar.rs");
    t.compile_fail("tests/ui/bad_range.rs");
}

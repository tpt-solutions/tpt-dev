//! Compile-fail tests for the `#[circuit]` front end.

#[test]
fn unsupported_syntax_is_a_compile_error() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}

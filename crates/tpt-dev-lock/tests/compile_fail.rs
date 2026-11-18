//! Lock-ordering mistakes must be rejected by the compiler.
#[test]
fn deadlock_prone_code_is_rejected() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}

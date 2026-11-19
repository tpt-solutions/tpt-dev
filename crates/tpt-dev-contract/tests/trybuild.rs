//! Compile-time behaviour: refuted clauses fail to compile, and proofs compile.
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/refuted.rs");
    t.compile_fail("tests/ui/refuted_old.rs");
    t.pass("tests/ui/proven.rs");
}

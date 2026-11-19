//! Deadlocking machines in the const-checkable subset must fail to compile.
#[test]
fn deadlocking_machines_are_rejected() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}

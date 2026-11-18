//! Traits the macro cannot mock must fail with a clear error.
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}

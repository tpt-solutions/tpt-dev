//! Behaviour of the contract macros. Proven clauses add no check, and the
//! others are checked in debug builds (the test profile).

#![allow(clippy::unwrap_used, clippy::panic, clippy::let_and_return)]

// Path-style attributes are used so that each attribute name is matched by its last segment.
// Proven at compile time: the body is linear and the clause follows from it.
#[tpt_dev_contract::ensures(result >= x)]
fn plus_one(x: i64) -> i64 {
    x + 1
}

#[tpt_dev_contract::requires(a < b)]
#[tpt_dev_contract::ensures(result == b - a)]
fn diff(a: i64, b: i64) -> i64 {
    let d = b - a;
    d
}

// Proven, with a precondition that the proof uses.
#[tpt_dev_contract::requires(x > 0)]
#[tpt_dev_contract::ensures(result > 1)]
fn at_least_two(x: i64) -> i64 {
    let y = x + 1;
    y
}

// Attribute order does not matter.
#[tpt_dev_contract::ensures(result >= 0)]
#[tpt_dev_contract::requires(x >= 0)]
fn copy_nonneg(x: i64) -> i64 {
    x
}

// Not linear, so the proof cannot run and the check happens at run time.
#[tpt_dev_contract::ensures(result < 100)]
fn square(x: i64) -> i64 {
    x * x
}

#[tpt_dev_contract::requires(x >= 0)]
fn id(x: i64) -> i64 {
    x
}

// Not an integer type, so the contracts are checked at run time.
#[tpt_dev_contract::requires(x >= 0.0)]
fn half(x: f64) -> f64 {
    x / 2.0
}

// The requires clauses are contradictory, so nothing is proven and the checks stay.
#[tpt_dev_contract::requires(x > 5)]
#[tpt_dev_contract::requires(x < 3)]
#[tpt_dev_contract::ensures(result == 7)]
fn vacuous(x: i64) -> i64 {
    x
}

// Integer inputs make the requires clauses unsatisfiable, but over the reals
// they have a fractional model. No compile error must be raised.
#[tpt_dev_contract::requires(x > 0)]
#[tpt_dev_contract::requires(x < 1)]
#[tpt_dev_contract::ensures(result > 1)]
fn no_integer_solution(x: i64) -> i64 {
    x
}

// `old(x)` is the entry value of `x`. Proven, since the solver equates the
// snapshot with the parameter before the body runs.
#[tpt_dev_contract::requires(x >= 0)]
#[tpt_dev_contract::ensures(result == old(x) + 1)]
fn succ_of_entry(x: i64) -> i64 {
    let y = x + 1;
    y
}

// The body reassigns `x`, so the proof does not apply. The run-time check must
// compare against the entry value, not the reassigned one.
#[tpt_dev_contract::ensures(result == old(x) + 1)]
fn bump_in_place(mut x: i64) -> i64 {
    x += 1;
    x
}

// Wrong on purpose: the run-time `old(x)` check must reject it.
#[tpt_dev_contract::ensures(result == old(x) + 2)]
fn bump_wrong(mut x: i64) -> i64 {
    x += 1;
    x
}

#[test]
fn proven_contracts_hold() {
    assert_eq!(plus_one(4), 5);
    assert_eq!(diff(2, 9), 7);
    assert_eq!(at_least_two(1), 2);
    assert_eq!(copy_nonneg(3), 3);
    assert_eq!(succ_of_entry(6), 7);
}

#[test]
fn old_uses_the_entry_value_at_run_time() {
    assert_eq!(bump_in_place(4), 5);
}

#[test]
#[should_panic(expected = "`ensures` clause violated: result == old(x) + 2")]
fn old_is_checked_at_run_time() {
    let _ = bump_wrong(4);
}

#[test]
fn runtime_ensures_holds_when_satisfied() {
    assert_eq!(square(9), 81);
}

#[test]
#[should_panic(expected = "`ensures` clause violated")]
fn runtime_ensures_is_checked() {
    let _ = square(10);
}

#[test]
#[should_panic(expected = "`requires` clause violated")]
fn requires_is_checked_on_entry() {
    let _ = id(-1);
}

#[test]
fn requires_passes_for_valid_inputs() {
    assert!((half(3.0) - 1.5).abs() < 1e-12);
}

#[test]
#[should_panic(expected = "`requires` clause violated")]
fn contradictory_requires_still_checked() {
    let _ = vacuous(4);
}

#[test]
fn fractional_only_models_do_not_fail_compilation() {
    // Compiling this file is the test. The function is never called.
    let _ = no_integer_solution;
}

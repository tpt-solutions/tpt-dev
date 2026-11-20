//! `return` in a body: the proof covers every return point, and the run-time
//! checks see the value of each early return.

#![allow(clippy::unwrap_used, clippy::panic)]

// Proven: both the early return and the final expression meet the clause.
#[tpt_dev_contract::ensures(result >= 0)]
fn abs_val(x: i64) -> i64 {
    if x < 0 {
        return -x;
    }
    x
}

// Proven, with the return on the else branch and the value of the then branch as the tail.
#[tpt_dev_contract::ensures(result >= 1)]
fn clamp_one(x: i64) -> i64 {
    if x >= 1 {
        return x;
    } else {
        1
    }
}

// Not linear, so the proof cannot run. The early return must still be checked at run time.
#[tpt_dev_contract::ensures(result > 0)]
fn early_zero(x: i64) -> i64 {
    if x == 0 {
        return 0;
    }
    x * x
}

// Not linear. The final expression must still be checked too.
#[tpt_dev_contract::ensures(result < 100)]
fn early_big(x: i64) -> i64 {
    if x > 50 {
        return x * 3;
    }
    x * x
}

// Not linear, and correct: the checks pass on both paths.
#[tpt_dev_contract::ensures(result >= 0)]
fn early_square(x: i64) -> i64 {
    if x == 0 {
        return 0;
    }
    x * x
}

#[test]
fn proven_early_returns_hold() {
    assert_eq!(abs_val(-3), 3);
    assert_eq!(abs_val(4), 4);
    assert_eq!(clamp_one(0), 1);
    assert_eq!(clamp_one(7), 7);
}

#[test]
fn runtime_early_returns_hold_when_satisfied() {
    assert_eq!(early_square(0), 0);
    assert_eq!(early_square(3), 9);
    assert_eq!(early_big(2), 4);
}

#[test]
#[should_panic(expected = "`ensures` clause violated: result > 0")]
fn early_return_is_checked_at_run_time() {
    let _ = early_zero(0);
}

#[test]
#[should_panic(expected = "`ensures` clause violated: result < 100")]
fn early_return_value_is_checked() {
    let _ = early_big(60);
}

// A `let` inside a block ends with the block. If the inner `y` leaked, the
// proof would relate `result` to 100 and fail to compile.
#[tpt_dev_contract::ensures(result == x)]
fn scoped(x: i64) -> i64 {
    let y = x;
    {
        let y = 100;
        let _ = y;
    }
    y
}

#[test]
fn block_bindings_do_not_leak() {
    assert_eq!(scoped(4), 4);
}

//! Contracts on functions, plus a solver check of a property.
//!
//! `diff` has a contract the compiler can prove, so it carries no runtime
//! check. `square` has a contract the compiler cannot prove, so it is checked
//! on each call in debug builds. The solver check at the end asks whether a
//! claimed property holds, independently of the contracts.
//!
//! Run with: `cargo run -p tpt-dev --example verified_contract --features contract,verify`

use tpt_dev::solver::Session;
use tpt_dev::{ensures, requires};

/// The difference of two numbers, with `a < b` required and proved.
#[requires(a < b)]
#[ensures(result == b - a)]
fn diff(a: i64, b: i64) -> i64 {
    let d = b - a;
    d
}

/// Not linear, so the `ensures` clause is checked at run time in debug builds.
#[ensures(result < 100)]
fn square(x: i64) -> i64 {
    x * x
}

fn main() {
    println!("diff(2, 9) = {}", diff(2, 9));
    println!("square(5) = {}", square(5));

    // Does `x > 1` and `x < 0` have a real solution? It must not.
    let mut session = Session::new();
    let answer = session
        .run(
            "(declare-const x Real)
             (assert (> x 1))
             (assert (< x 0))
             (check-sat)",
        )
        .expect("script is valid");
    print!("x > 1 and x < 0: {answer}");
}

//! A linear body that the solver refutes. Compilation must fail with a counterexample.
//! Path-style attributes avoid unused-import warnings when two contract attributes are combined.

#[tpt_dev_contract::requires(x >= 5)]
#[tpt_dev_contract::ensures(result > 100)]
fn bump(x: i64) -> i64 {
    let y = x + 1;
    y
}

fn main() {
    let _ = bump(5);
}

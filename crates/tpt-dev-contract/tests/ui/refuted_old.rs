//! A linear body that breaks an `old()` postcondition. Compilation must fail
//! with a counterexample for the entry value of `x`.

#[tpt_dev_contract::requires(x >= 0)]
#[tpt_dev_contract::ensures(result == old(x))]
fn bump(x: i64) -> i64 {
    let y = x + 1;
    y
}

fn main() {
    let _ = bump(0);
}

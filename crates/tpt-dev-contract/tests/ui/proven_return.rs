//! Early returns that the solver proves on every path. Compilation must succeed.

#[tpt_dev_contract::requires(x >= 0)]
#[tpt_dev_contract::ensures(result >= x)]
fn bump_or_keep(x: i64) -> i64 {
    if x > 100 {
        return x;
    }
    x + 1
}

fn main() {
    let _ = bump_or_keep(3);
}

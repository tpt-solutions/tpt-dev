//! A refuted clause on an early return. The counterexample must name the input.

#[tpt_dev_contract::ensures(result > 0)]
fn clamp_zero(x: i64) -> i64 {
    if x < 0 {
        return 0;
    }
    x
}

fn main() {
    let _ = clamp_zero(1);
}

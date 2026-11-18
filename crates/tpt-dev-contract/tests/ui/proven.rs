//! A proof over linear code with requires clauses. Must compile.

#[tpt_dev_contract::requires(a < b)]
#[tpt_dev_contract::ensures(result == b - a)]
#[tpt_dev_contract::ensures(result > 0)]
fn diff(a: i64, b: i64) -> i64 {
    let d = b - a;
    d
}

fn main() {
    assert_eq!(diff(1, 4), 3);
}

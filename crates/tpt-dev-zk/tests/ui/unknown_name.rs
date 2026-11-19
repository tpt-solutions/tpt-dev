use tpt_dev_zk::circuit;

#[circuit]
fn ghost(x: i64) -> i64 {
    x + y
}

fn main() {}

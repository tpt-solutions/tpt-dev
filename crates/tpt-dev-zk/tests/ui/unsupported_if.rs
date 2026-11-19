use tpt_dev_zk::circuit;

#[circuit]
fn choose(x: i64) -> i64 {
    if x > 0 { x } else { 0 }
}

fn main() {}

//! Writes a sample circuit and its witness, for checking with snarkjs.
//!
//! Run with `cargo run -p tpt-dev-zk --example export_r1cs -- <dir>`. The
//! directory receives `circuit.r1cs` and `witness.wtns`.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use tpt_dev_zk::{Circuit, circuit};

#[circuit]
#[allow(dead_code)]
fn poly(x: i64, k: i64) -> i64 {
    let t = x * x;
    t * 3 + k - (-x)
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap());
    let c = Circuit::new();
    let x = c.private_input(7);
    let k = c.public_input(-2);
    let _ = c.output(&poly_circuit(x, k));
    c.check().unwrap();
    std::fs::write(dir.join("circuit.r1cs"), c.to_r1cs()).unwrap();
    std::fs::write(dir.join("witness.wtns"), c.to_wtns()).unwrap();
}

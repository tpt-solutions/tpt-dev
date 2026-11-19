//! The `#[circuit]` front end: the generated circuit function agrees with the native one.

#![allow(clippy::unwrap_used)]

use tpt_dev_zk::{Circuit, circuit};

#[circuit]
fn poly(x: i64, k: i64) -> i64 {
    let t = x * x;
    t * 3 + k - (-x)
}

#[circuit]
fn blocks(a: i64, b: i64) -> i64 {
    let s = {
        let inner = a - b;
        inner * (a + 1)
    };
    s * s - 7
}

fn run_both(f: impl Fn(&Circuit, i64, i64) -> (usize, i64), native: impl Fn(i64, i64) -> i64) {
    for (x, k) in [(0i64, 0i64), (2, 5), (-4, 7), (9, -3), (13, 13)] {
        let c = Circuit::new();
        let (out, _) = f(&c, x, k);
        c.check().unwrap();
        assert_eq!(c.value(out), native(x, k), "inputs {x}, {k}");
    }
}

#[test]
fn the_generated_circuit_matches_native_execution() {
    run_both(
        |c, x, k| {
            let wx = c.private_input(x);
            let wk = c.public_input(k);
            (c.output(&poly_circuit(wx, wk)), 0)
        },
        poly,
    );
    run_both(
        |c, a, b| {
            let wa = c.private_input(a);
            let wb = c.public_input(b);
            (c.output(&blocks_circuit(wa, wb)), 0)
        },
        blocks,
    );
}

#[test]
fn constants_and_negation_are_free_only_products_cost_constraints() {
    let c = Circuit::new();
    let x = c.private_input(5);
    let k = c.public_input(2);
    let out = c.output(&poly_circuit(x, k));
    let _ = out;
    // x * x is the one product; t * 3, k, and -x are linear. The output adds one more.
    assert_eq!(c.num_constraints(), 2);
    c.check().unwrap();
}

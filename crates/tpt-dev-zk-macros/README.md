# tpt-dev-zk-macros

The `#[circuit]` attribute for [`tpt-dev-zk`](../tpt-dev-zk). It is re-exported there, so use it as `tpt_dev_zk::circuit`.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

A proc-macro crate, so it runs on the host only and is excluded from the bare-metal CI build.

## What it does

Applied to a function over `i64`, the attribute keeps the function and writes `<name>_circuit`, generic over `Ring`. The new function has the same body, with each integer literal made in the circuit and each name cloned. Calling it with wires records the circuit.

```rust
use tpt_dev_zk::circuit;

#[circuit]
fn poly(x: i64, k: i64) -> i64 {
    let t = x * x;
    t * 3 + k - (-x)
}
// Generates poly_circuit<T: Ring>(x: T, k: T) -> T.
```

## Supported syntax

Integer literals, parameters and earlier `let` names, parentheses, `+`, `-`, `*`, unary `-`, and nested blocks whose last expression is the value. Everything else is a compile error, with a message that names the construct.

## Not supported

- Generic functions, methods, and parameters that are not plain `i64` names.
- Division, comparisons, `if`, `match`, loops, and calls to other functions.
- `let` statements with patterns other than a plain name, and `let`-`else`.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

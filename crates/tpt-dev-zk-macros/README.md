# tpt-dev-zk-macros

`#[circuit]` attribute for tpt-dev-zk: writes a circuit version of an arithmetic function.

## Overview

This is the proc-macro half of [`tpt-dev-zk`](https://docs.rs/tpt-dev-zk). Applied to a function over `i64`, `#[circuit]` keeps the function and writes a second one, `<name>_circuit`, generic over `tpt_dev_zk::Ring`. The new function has the same body, with each integer literal made in the circuit and each name cloned. Calling it with wires records the circuit.

You normally do not depend on this crate directly. It is re-exported from `tpt-dev-zk` as `tpt_dev_zk::circuit`, and the generated code refers to `::tpt_dev_zk::Ring`, so the crate that uses the attribute must depend on `tpt-dev-zk`.

> **Not independently audited.** The macro only rewrites arithmetic into `Ring` operations. It does not check or prove anything about the circuit, and the scope notes in `tpt-dev-zk` apply.

## Features

- **Generated circuit function:** `poly` gets a sibling `poly_circuit<__T: Ring>(...) -> __T`, so one source gives a native function and a circuit.
- **Clear errors:** unsupported syntax is a compile error with a message that names the construct.
- **Original kept:** the annotated function is emitted unchanged.

## Installation

```sh
cargo add tpt-dev-zk
```

Use the attribute through `tpt-dev-zk`. To depend on the macro crate alone, run `cargo add tpt-dev-zk-macros`, but the generated code still needs `tpt-dev-zk` in scope.

This is a proc-macro crate (`[lib] proc-macro = true`), so it runs on the host only. CI excludes it from the `thumbv7em-none-eabihf` build. It has no feature flags. Its dependencies are `syn`, `quote` and `proc-macro2`.

## Quick start

```rust
use tpt_dev_zk::{Circuit, circuit};

#[circuit]
fn poly(x: i64, k: i64) -> i64 {
    let t = x * x;
    t * 3 + k - (-x)
}
// Generates poly_circuit<T: Ring>(x: T, k: T) -> T.

let c = Circuit::new();
let x = c.private_input(2);
let k = c.public_input(1);
let out = c.output(&poly_circuit(x, k));
c.check().unwrap();
assert_eq!(c.value(out), poly(2, 1));
```

## Design notes and guarantees

- **Same body.** The generated function repeats the original body. Each literal becomes `Ring::lit` on a value taken from the first parameter, and each name is cloned.
- **Visibility is kept.** `<name>_circuit` has the same visibility as the original.
- **Unary minus** is written as `0 - x` in the circuit.
- **Compile-fail tests** for this front end live in `tests/ui/` of `tpt-dev-zk`.

## Limitations and scope

- **Supported syntax:** integer literals, parameters and earlier `let` names, parentheses, `+`, `-`, `*`, unary `-`, and nested blocks whose last expression is the value. Everything else is a compile error.
- **Not supported:** generic functions, methods, and parameters that are not plain `i64` names. The return type must be `i64`, and the function needs at least one parameter.
- **Also rejected:** division, comparisons, `if`, `match`, loops, and calls to other functions, and `let` statements with patterns other than a plain name or with `let`-`else`.
- **No arguments.** `#[circuit]` takes none.
- **No analysis of its own.** It does not bound values or check for overflow in the native function.

## Related crates

- Used by [`tpt-dev-zk`](https://docs.rs/tpt-dev-zk), which re-exports `circuit` and defines `Ring`, `Wire` and `Circuit`.
- Depends on no other workspace crate.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

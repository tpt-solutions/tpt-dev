# tpt-dev-zk

Rust-to-zero-knowledge circuit transpiler targeting R1CS.

## Overview

`tpt-dev-zk` turns ordinary Rust arithmetic into a rank-1 constraint system (R1CS) over the BN254 scalar field, and writes it in the binary `.r1cs` format that circom-compatible tools read. It also produces the matching witness. Proof systems stay external: this crate makes constraints and witnesses, and nothing else.

The same source runs two ways. A function written generically over the `Ring` trait runs natively on `i64` and records a circuit on `Wire`, so tests can compare both results. A plain `i64` function marked `#[circuit]` (from [`tpt-dev-zk-macros`](https://docs.rs/tpt-dev-zk-macros), re-exported here) gets a generated `<name>_circuit` that is generic over `Ring`. The crate is `no_std` with `alloc`.

> **Not independently audited.** This crate is experimental. Do not use it to protect real value. See the limits below for what has and has not been checked.

## Features

- **Front end:** a function written generically over `Ring` runs natively on `i64`, and runs as a circuit on `Wire`. The same source gives both results. Addition and subtraction add no constraint. Multiplying two non-constant wires adds one constraint. Constants are free.
- **`#[circuit]`:** generates the `Ring`-generic version of an `i64` function from its body.
- **Circuit:** `Circuit` holds the constraints and the witness. It has private and public inputs, public outputs, and equality assertions. `check` returns the index of the first constraint the witness breaks.
- **Export:** `Circuit::to_r1cs` writes the iden3 `.r1cs` binary format, `Circuit::witness` gives the witness in the same wire order, and `Circuit::to_wtns` writes the `.wtns` witness format that snarkjs reads.
- **BN254 field:** `F` is the scalar field element type, with `MODULUS_LE` as the modulus in little-endian bytes.

## Installation

```sh
cargo add tpt-dev-zk
```

The crate is `#![no_std]` and needs `alloc`. It depends on `tpt-dev-zk-macros`, a proc-macro crate, which is built for the host. CI builds `tpt-dev-zk` itself for `thumbv7em-none-eabihf`.

| Feature | Enables | Default |
| ------- | ------- | ------- |
| `std`   | Links `std`. The library has no `std`-only code today, so the flag changes nothing in the public API. | off |

## Quick start

```rust
use tpt_dev_zk::{Circuit, Ring};

fn square_plus<T: Ring>(x: T, k: T) -> T {
    x.clone() * x.clone() + k
}

let c = Circuit::new();
let x = c.private_input(6);
let k = c.public_input(4);
let out = c.output(&square_plus(x, k));
c.check().unwrap();
assert_eq!(c.value(out), 40);
assert_eq!(c.num_constraints(), 2); // the product, and the output
```

With the attribute, the generic function is written for you:

```rust
use tpt_dev_zk::{Circuit, circuit};

#[circuit]
fn poly(x: i64, k: i64) -> i64 {
    let t = x * x;
    t * 3 + k - (-x)
}

let c = Circuit::new();
let x = c.private_input(7);
let k = c.public_input(-2);
let out = c.output(&poly_circuit(x, k));
c.check().unwrap();
assert_eq!(c.value(out), poly(7, -2));
```

## More examples

- `export_r1cs`: builds the `poly` circuit above and writes `circuit.r1cs` and `witness.wtns` into a directory, for checking with snarkjs.

  ```sh
  cargo run -p tpt-dev-zk --example export_r1cs -- <dir>
  snarkjs r1cs info <dir>/circuit.r1cs
  snarkjs wtns check <dir>/circuit.r1cs <dir>/witness.wtns
  ```

## Design notes and guarantees

- **Constraints match the computation.** Each product is constrained as it is computed, so `check` fails for a witness that does not satisfy the circuit. The tests confirm this for a false equality.
- **One cost model.** `+`, `-` and constants add no constraint. Each product of two non-constant wires adds one constraint and one wire. `Circuit::output` adds one more constraint per output.
- **Wire order follows the format.** Export renumbers wires as the constant one, public outputs, public inputs, private inputs, then internal wires.
- **Field arithmetic.** `F` is in Montgomery form on four 64-bit limbs, and every stored value is reduced below the modulus r = 21888242871839275222246405745257275088548364400416034343698204186575808495617. The tests check the operations against independently computed big-integer vectors (`tests/fixtures/bn254_vectors.txt`).
- **Checked with snarkjs 0.7.4.** `snarkjs r1cs info` reads the exported file and reports the curve as `bn-128`, with the wire, constraint and input counts of the sample circuit. `snarkjs wtns check` accepts the witness, and rejects it once one byte is changed. snarkjs is not part of this repository.

## Limitations and scope

- **Not audited.** No independent security review has been done. Soundness of any proof depends on the proof system, which is not included here. Any proof built on these circuits needs an external backend that has been reviewed.
- **No proof generation.** The crate produces constraints and witnesses. Proving and verifying are left to external backends.
- **Limits of the snarkjs check.** Circom's own compiler output and proof generation are not tested.
- **`Circuit::value` truncates.** Values that do not fit in 64 bits are reported as their low 64 bits.
- **Front end subset.** Both ways in support integer literals, names, `let` bindings, parentheses, `+`, `-`, `*` and unary `-`. Division, comparisons, `if`, loops and calls to other functions are rejected by `#[circuit]` at compile time, because a traced circuit has no value to branch on. Inputs and outputs are `i64`.
- **The witness is private.** `Circuit::witness` and `to_wtns` include private inputs. Do not send them to a verifier.
- **The macro needs a host build.** `tpt-dev-zk-macros` is a proc-macro crate, so CI builds it for the host only.
- **Timing behaviour is not documented.** The crate makes no constant-time claim for field arithmetic.

## Related crates

- Depends on [`tpt-dev-zk-macros`](https://docs.rs/tpt-dev-zk-macros), which provides `#[circuit]`.
- No workspace crate depends on it.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

# tpt-dev-zk

Turns ordinary Rust arithmetic into a rank-1 constraint system (R1CS), and exports it in the binary `.r1cs` format. Proof systems stay external.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

- **Front end:** a function written generically over `Ring` runs natively on `i64`, and runs as a circuit on `Wire`. The same source gives both results, so tests can compare them. Addition and subtraction add no constraint. Multiplying two non-constant wires adds one constraint. Constants are free.
- **Circuit:** `Circuit` holds the constraints and the witness. It has private and public inputs, public outputs, and equality assertions. `check` finds the first constraint the witness breaks.
- **Export:** `Circuit::to_r1cs` writes the iden3 `.r1cs` binary format, and `Circuit::witness` gives the witness in the same wire order.

## Example

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

## Limits

- **Small field.** The arithmetic is modulo 2^61 - 1, so it fits in `u128`. Real proof systems use a 254-bit field such as BN254's scalar field. The `.r1cs` header records the modulus, so the file is well formed, but circom and snarkjs will not accept these circuits as BN254 circuits.
- **Format not checked against circom.** The encoder is checked by decoding its own output, and the tests confirm that the encoded constraints hold for the encoded witness. Nothing here has been loaded by circom or snarkjs.
- **Tracing front end, not a proc macro.** A Rust function is supported when it is written generically over `Ring`, and uses only `+`, `-` and `*`. There is no division, comparison, or branching on a wire value, because a traced circuit has no value to branch on. A proc macro that reads ordinary `fn` items is not written.
- **No proof generation.** The crate produces constraints and witnesses. Proving and verifying are left to external backends, as decided.
- **Witness is private.** `Circuit::witness` includes private inputs. Do not send it to a verifier.

## Safety review

- **Constraints match the computation.** Each product is constrained as it is computed, so `check` fails for a witness that does not satisfy the circuit. The tests confirm this for a false equality.
- **The field is not secure.** Its size is far too small for soundness. Treat this crate's proofs as research output until it is moved to a 254-bit field.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

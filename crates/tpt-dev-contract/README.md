# tpt-dev-contract

`#[requires]` and `#[ensures]` attribute macros for design-by-contract on integer functions. Where the solver can prove an `ensures` clause at compile time, it adds no run-time check. Where it refutes one, the build fails with a counterexample.

- **`requires`** is checked on entry, in debug builds. Callers must satisfy it.
- **`ensures`** is checked on return, in debug builds, unless it was proven.
- **Compile-time proof:** for a body made of `let` statements and one final expression in linear arithmetic, the solver checks that the `requires` clauses and the negated `ensures` clause are unsatisfiable.
- **Counterexample errors:** if a refutation uses integer inputs, the build fails and names the inputs that break the clause.

## Example

```rust
#[tpt_dev_contract::requires(a < b)]
#[tpt_dev_contract::ensures(result > 0)]
fn gap(a: i64, b: i64) -> i64 {
    let d = b - a;
    d
}
```

The `ensures` clause is proven, so it adds no check. A version with `ensures(result > 10)` fails to compile and reports a counterexample.

## Supported subset

- Integer types: `i8` to `i128`, `isize`, `u8` to `u128`, `usize`. Floats are not supported, because real arithmetic does not model rounding.
- Expressions: integer and boolean literals, names, `+`, `-`, `*` where one side is constant, unary `-` and `!`, comparisons, `&&`, `||`, and parentheses.
- Bodies: `let` statements and a final expression. Other statements, `return`, `if` and calls fall back to run-time checks.
- `result` names the return value in `ensures`.

## Limits

- **Integer arithmetic is treated as unbounded.** A proof does not account for overflow. Overflow still panics in debug builds, as usual.
- **Real models are not counterexamples.** The solver works over the reals, so a refutation is reported only when every input in its model is an integer. Otherwise the check stays at run time.
- **Only the function body is analysed.** Callers' arguments are not checked at compile time. That needs a whole-program pass, which is not written.
- **Stacked contract attributes.** Use path-style attributes (`#[tpt_dev_contract::ensures(...)]`) when combining them. Each macro takes over the other contract attributes on the function, and path-style names avoid unused-import warnings.
- **Early `return` skips `ensures`.** The return-value check wraps the body's final value only.
- **Host only.** This is a proc-macro crate, so it cannot target `thumbv7em-none-eabihf`. CI excludes it from the bare-metal build.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

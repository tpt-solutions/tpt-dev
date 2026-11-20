# tpt-dev-contract

`#[requires]` and `#[ensures]` macros for compile-time provable design-by-contract.

## Overview

`tpt-dev-contract` adds preconditions and postconditions to integer functions. Both are checked at run time in debug builds. In addition, when a function body is simple enough, the macro asks [`tpt-dev-solver`](https://docs.rs/tpt-dev-solver) to prove each `ensures` clause during compilation. A proven clause adds no run-time check. A clause the solver refutes with an integer counterexample stops the build and names the inputs that break it.

It is for small arithmetic functions where a contract can be settled by linear reasoning. It is a proc-macro crate, so it runs on the host at compile time and is not itself `no_std`. The code it generates uses `core::debug_assert!`, so the functions you annotate can still be used in `no_std` crates.

## Features

- **`requires`:** checked on entry with `debug_assert!`. Callers must satisfy it.
- **`ensures`:** checked on return with `debug_assert!`, unless it was proven at compile time.
- **Compile-time proof:** for a body of `let` statements, `if` statements, `return` and a final expression, in linear arithmetic, the solver checks that the `requires` clauses and the negated `ensures` clause are unsatisfiable. Every return point is covered.
- **Counterexample errors:** if a refutation uses only integer inputs, the build fails and names the inputs that break the clause.
- **`old(x)` in `ensures`:** the value a parameter had on entry. The run-time check copies it before the body runs.
- **Stacking:** several `requires` and `ensures` attributes can sit on one function.

## Installation

```sh
cargo add tpt-dev-contract
```

This is a proc-macro crate (`[lib] proc-macro = true`). It is built for the host only. CI excludes it from the `thumbv7em-none-eabihf` build. It has no feature flags.

Its dependencies are `syn`, `quote`, `proc-macro2` and `tpt-dev-solver`.

## Quick start

```rust
#[tpt_dev_contract::requires(a < b)]
#[tpt_dev_contract::ensures(result > 0)]
#[tpt_dev_contract::ensures(result == b - a)]
fn gap(a: i64, b: i64) -> i64 {
    let d = b - a;
    d
}

assert_eq!(gap(1, 4), 3);
```

Both `ensures` clauses are proven, so they add no run-time check. A version with `ensures(result > 10)` fails to compile with a message like this:

```text
error: `ensures` clause `result > 100` is false: counterexample: x = 5
```

`old` refers to a parameter's value on entry:

```rust
#[tpt_dev_contract::ensures(result == old(n) + 1)]
fn bump(mut n: i64) -> i64 {
    n = n + 1;
    n
}
```

This body has a statement that is not a `let`, so the clause is not proven. It is checked at run time against the entry value of `n`.

## Supported subset

- **Integer types:** `i8` to `i128`, `isize`, `u8` to `u128`, `usize`. Floats are not supported, because real arithmetic does not model rounding.
- **Expressions:** integer and boolean literals, names, `+`, `-`, `*` where one side is constant, unary `-` and `!`, comparisons, `&&`, `||`, parentheses, and `old(x)`.
- **`result`:** names the return value in `ensures`.
- **`old(x)`:** allowed only in `ensures`, for a parameter `x` of integer type. The proof treats it as a fresh snapshot variable equal to `x` before the body runs. The run-time check copies `x` before the body, so a body that reassigns a `mut` parameter is still checked against the entry value. `old` in `requires` is a compile error.
- **Bodies:** `let` statements, `if`/`else` (with or without `return` in either branch), nested blocks, `return expr;` and a final expression. Each `if` splits the proof into two paths, each with its condition. Loops, assignments, calls and other statements fall back to run-time checks, as does a body with more than 64 return points.
- **Early `return`:** the proof treats the return value as a variable constrained on each path. The run-time check wraps the body in a closure, so an early `return` is checked against `ensures` too.

## Design notes and guarantees

- **A proof is only made when every parameter is a plain integer.** Otherwise every clause stays a run-time check.
- **Vacuous proofs are avoided.** If the `requires` clauses are unsatisfiable, nothing is proven and all `ensures` clauses stay run-time checks.
- **Only integer counterexamples are reported.** The solver works over the reals. A refutation becomes a compile error only when every input in its model is an integer. Otherwise the clause stays a run-time check.
- **Clauses the translator cannot read stay run-time checks.** An unsupported expression never produces a proof.
- **Compile-fail behaviour is tested.** `tests/ui/` holds `trybuild` cases for a proven contract, a refuted one and a refuted `old` clause.

## Limitations and scope

- **Integer arithmetic is treated as unbounded.** A proof does not account for overflow. Overflow still panics in debug builds, as usual.
- **Only the function body is analysed.** Callers' arguments are not checked at compile time. That needs a whole-program pass, which is not written.
- **Checks are debug-only.** Both `requires` and `ensures` use `debug_assert!`, so release builds do not check clauses that were not proven.
- **Stacked contract attributes.** Use path-style attributes (`#[tpt_dev_contract::ensures(...)]`) when combining them. Each macro takes over the other contract attributes on the function, and path-style names avoid unused-import warnings.
- **`old()` needs a `Copy` parameter.** The run-time snapshot copies the parameter on entry. A non-`Copy` parameter used under `old` fails to compile in the generated code.
- **Host only.** This is a proc-macro crate, so it cannot target `thumbv7em-none-eabihf`.

## Related crates

- Depends on [`tpt-dev-solver`](https://docs.rs/tpt-dev-solver) for the proofs.
- No workspace crate depends on it.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

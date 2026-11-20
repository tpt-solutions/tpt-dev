# tpt-dev-solver

Pure-Rust QF_LRA SMT solver that removes the need for external Z3 binaries.

## Overview

`tpt-dev-solver` decides satisfiability of quantifier-free linear real arithmetic (QF_LRA): Boolean combinations of linear inequalities over real variables. It exists so that the verification tools in this workspace can ask arithmetic questions without shelling out to Z3 or linking a C++ library. Everything is Rust, with no dependencies, and it builds for `no_std` targets that have a global allocator.

The arithmetic is exact. Values are rationals on checked `i128`, and any overflow produces `Unknown` instead of a wrong answer. A CDCL SAT core handles the Boolean structure and a general simplex with delta-rationals (for strict bounds) handles the linear part. Every `Sat` answer has a model, and unsatisfiable answers can come with checkable evidence: unsat cores, Farkas certificates, and DRUP-style proofs.

The solver is the shared engine behind several sibling crates: [`tpt-dev-test`](https://docs.rs/tpt-dev-test) (constrained generation and invariant search), [`tpt-dev-fuzz`](https://docs.rs/tpt-dev-fuzz) (solver-derived fuzz seeds), [`tpt-dev-contract`](https://docs.rs/tpt-dev-contract) (compile-time proof of `#[ensures]` clauses) and [`tpt-dev-automata`](https://docs.rs/tpt-dev-automata) (deadlock analysis of guarded state machines).

## Features

- **Exact arithmetic:** rationals on checked `i128`. Overflow returns `Unknown`, never a wrong answer.
- **DPLL(T):** a CDCL SAT core drives the Boolean structure. A general simplex with delta-rationals (for strict bounds) checks the linear part.
- **Models:** every `Sat` answer comes with a model, read with `Solver::value`. The randomized test suite checks each model against the formula.
- **Incremental solving:** `push` and `pop` keep the encoding, the learned clauses and the simplex tableau. Assertions are passed as SAT assumptions, so `pop` retracts them without invalidating anything learned.
- **Unsat cores:** `unsat_core` returns a minimal set of assertions that is still unsatisfiable.
- **Farkas certificates:** `unsat_certificate` returns a checkable proof for conjunctions of linear literals. `check_farkas` verifies it without the solver. The format is documented on `Farkas`.
- **Proofs for Boolean structure:** `unsat_proof` returns a `Proof` for any Boolean combination of linear atoms. It is a DRUP-style clause list whose theory lemmas each carry a Farkas certificate. `check_proof` verifies it by unit propagation and `check_farkas`, and never calls the solver.
- **Budgets:** `Budget` caps formula nodes, clauses and simplex pivots. A check that reaches a cap returns `Unknown`, and the solver stays usable.
- **Fixed-capacity arena:** `FixedArena` stores formulas inline in const-generic arrays, with no heap allocation in the store.
- **SMT-LIB subset:** `Session` runs scripts with `declare-fun`, `declare-const`, `assert`, `check-sat`, `get-model`, `push`, `pop` and `exit`. `set-logic`, `set-info` and `set-option` are accepted and ignored.

## Installation

```sh
cargo add tpt-dev-solver
```

The crate is `#![no_std]` and needs `alloc`. It has no dependencies. On bare metal it needs a global allocator. CI builds it for `thumbv7em-none-eabihf`.

| Feature | Enables | Default |
| ------- | ------- | ------- |
| `std`   | Links `std`. The library has no `std`-only code today, so the flag changes nothing in the public API. | off |

## Quick start

Build formulas in the solver's `Arena`, assert them, and check:

```rust
use tpt_dev_solver::{Linear, Rational, Solver, Status, check_farkas, check_proof};

let mut s = Solver::new();
let x = s.declare("x");
let y = s.declare("y");
let one = Linear::constant(Rational::ONE);

// x + y <= 1, x >= 1, y >= 1 has no solution.
let sum = Linear::var(x).checked_add(&Linear::var(y)).unwrap();
let a = s.arena_mut().le(&sum, &one).unwrap();
let b = s.arena_mut().ge(&Linear::var(x), &one).unwrap();
let c = s.arena_mut().ge(&Linear::var(y), &one).unwrap();
for f in [a, b, c] {
    s.assert(f);
}
assert_eq!(s.check(), Ok(Status::Unsat));

// Evidence that can be checked without trusting the search.
let cert = s.unsat_certificate().unwrap().unwrap();
assert!(check_farkas(s.arena(), s.assertions(), &cert));
let proof = s.unsat_proof().unwrap().unwrap();
assert!(check_proof(s.arena(), s.assertions(), &proof));

// A satisfiable problem has a model.
let mut t = Solver::new();
let z = t.declare("z");
let f = t.arena_mut().ge(&Linear::var(z), &one).unwrap();
t.assert(f);
assert_eq!(t.check(), Ok(Status::Sat));
assert!(t.value(z).unwrap() >= Rational::ONE);
```

Or run an SMT-LIB script:

```rust
use tpt_dev_solver::Session;

let mut session = Session::new();
let out = session.run(
    "(declare-fun x () Real)
     (declare-fun y () Real)
     (assert (= (+ x y) 10))
     (assert (= (- x y) 4))
     (check-sat)
     (get-model)",
).unwrap();
assert!(out.starts_with("sat\n"));
```

## More examples

- `bench`: times `check` on random QF_LRA instances from fixed seeds, with the average and worst case per row. The simplex tableau is kept across theory checks, which cut the time on the largest disjunctive instance about fivefold.

  ```sh
  cargo run -p tpt-dev-solver --release --example bench
  ```

## Design notes and guarantees

- **No wrong answers from overflow.** Every rational operation is checked. An overflow anywhere in a check returns `Err(Unknown)`.
- **Evidence you can check separately.** `check_farkas` and `check_proof` take an arena and the assertions, and do not call the solver. `Solver::unsat_proof` also runs `check_proof` on its own output and returns `Unknown` if the proof fails, so a bad proof does not escape.
- **Bounded work on request.** `Budget` limits formula nodes and clauses (counted over the solver's life) and simplex pivots (per check). `Budget::UNLIMITED` is the default.
- **Allocation-free formula store.** `FixedArena` takes its capacity as const generics and returns `None` when full.
- **Retained state across `pop`.** Learned clauses are consequences of formulas already encoded, so they stay valid after `pop`.

## Limitations and scope

- **Theory checks run only on full assignments.** There is no theory propagation. Checking partial assignments was tried and was slower.
- **No restarts and no clause deletion.** Learned clauses accumulate for the life of a solver, so a long session should start a new `Solver` (or set `Budget::max_clauses`).
- **Memory is not reclaimed by `pop`.** Encodings and learned clauses stay. `Budget` limits them.
- **Farkas certificates cover only conjunctions of linear literals.** A problem with `or` or a compound `not` gets no certificate. Use `unsat_proof` for those.
- **The Tseitin encoder is tested, not proved.** `check_proof` rebuilds the clauses with the crate's encoder. Its tests enumerate every assignment for random and edge-case formulas of up to 16 CNF variables, and check that the CNF matches the formula exactly. Larger formulas are not covered by that check. The proof must be checked against the arena it was built from.
- **The `FixedArena` store is allocation-free, but the search is not.** Solving uses `alloc` for its working state.
- **Unsat cores are deletion-based.** They take one solver call per assertion.
- **Only real variables and linear terms.** Nonlinear products, `ite` and `distinct` are rejected.
- **Not yet cross-checked against the SMT-LIB corpus.** Testing uses a randomized oracle (DNF plus Fourier-Motzkin elimination) and unit tests.

## Related crates

Depends on no other workspace crate. Used by:

- [`tpt-dev-test`](https://docs.rs/tpt-dev-test): property testing and invariant search.
- [`tpt-dev-fuzz`](https://docs.rs/tpt-dev-fuzz): solver-seeded fuzzing (through `tpt-dev-test`).
- [`tpt-dev-contract`](https://docs.rs/tpt-dev-contract): `#[requires]` and `#[ensures]` proofs.
- [`tpt-dev-automata`](https://docs.rs/tpt-dev-automata): state machine analysis.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

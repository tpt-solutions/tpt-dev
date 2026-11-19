# tpt-dev-solver

Pure-Rust solver for quantifier-free linear real arithmetic (QF_LRA). It replaces external Z3 binaries for formal verification.

- **Exact arithmetic:** rationals on checked `i128`. Overflow returns `Unknown`, never a wrong answer.
- **DPLL(T):** a CDCL SAT core drives the Boolean structure. A general simplex with delta-rationals (for strict bounds) checks the linear part.
- **Models:** every `Sat` answer comes with a model. The randomized test suite checks each model against the formula.
- **Incremental:** `push` and `pop` keep the encoding, the learned clauses and the simplex tableau. Assertions are passed as SAT assumptions, so `pop` retracts them without invalidating anything learned.
- **Unsat cores:** `unsat_core` returns a minimal set of assertions that is still unsatisfiable.
- **Farkas certificates:** `unsat_certificate` returns a checkable proof for conjunctions of linear literals. `check_farkas` verifies it without the solver. The format is documented on `Farkas`.
- **Proofs for Boolean structure:** `unsat_proof` returns a `Proof` for any Boolean combination of linear atoms. It is a DRUP-style clause list whose theory lemmas each carry a Farkas certificate. `check_proof` verifies it by unit propagation and `check_farkas`, and never calls the solver. The format is documented in `proof.rs`.
- **Budgets:** `Budget` caps formula nodes, clauses and simplex pivots. A check that reaches a cap returns `Unknown`.
- **Fixed-capacity arena:** `FixedArena` stores formulas inline, with no heap allocation in the store.
- **SMT-LIB subset:** `Session` runs scripts with `declare-fun`, `assert`, `check-sat`, `get-model`, `push`, `pop` and `exit`.
- **`no_std` with `alloc`:** no dependencies. Needs a global allocator on bare metal.

## Benchmarks

`cargo run -p tpt-dev-solver --release --example bench` times random QF_LRA instances. The simplex tableau is kept across theory checks, which cut the time on the largest disjunctive instance about fivefold.

## Example

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

## Limits

- **Theory checks run only on full assignments.** There is no theory propagation. Checking partial assignments was tried and was slower.
- **No restarts and no clause deletion.** Learned clauses accumulate for the life of a solver, so a long session should start a new `Solver` (or set `Budget::max_clauses`).
- **Memory is not reclaimed by `pop`.** Encodings and learned clauses stay. `Budget` limits them.
- **Farkas certificates cover only conjunctions of linear literals.** A problem with `or` or a compound `not` gets no certificate. Use `unsat_proof` for those.
- **The Tseitin encoder is tested, not proved.** `check_proof` rebuilds the clauses with the crate's encoder. Its tests enumerate every assignment for random and edge-case formulas of up to 16 CNF variables, and check that the CNF matches the formula exactly. Larger formulas are not covered by that check. The proof must be checked against the arena it was built from.
- **The `FixedArena` store is allocation-free, but the search is not.** Solving uses `alloc` for its working state.
- **Unsat cores are deletion-based.** They take one solver call per assertion.
- **Only real variables and linear terms.** Nonlinear products, `ite` and `distinct` are rejected.
- **Not yet cross-checked against the SMT-LIB corpus.** Testing uses a randomized oracle (DNF plus Fourier-Motzkin elimination) and unit tests.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

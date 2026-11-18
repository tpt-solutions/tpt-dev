# tpt-dev-solver

Pure-Rust solver for quantifier-free linear real arithmetic (QF_LRA). It replaces external Z3 binaries for formal verification.

- **Exact arithmetic:** rationals on checked `i128`. Overflow returns `Unknown`, never a wrong answer.
- **DPLL(T):** a CDCL SAT core drives the Boolean structure. A general simplex with delta-rationals (for strict bounds) checks the linear part.
- **Models:** every `Sat` answer comes with a model. The randomized test suite checks each model against the formula.
- **Unsat cores:** `unsat_core` returns a minimal set of assertions that is still unsatisfiable.
- **SMT-LIB subset:** `Session` runs scripts with `declare-fun`, `assert`, `check-sat`, `get-model`, `push`, `pop` and `exit`.
- **`no_std` with `alloc`:** no dependencies. Needs a global allocator on bare metal.

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

- **Assertions are re-solved on each check.** `push` and `pop` change the assertion set, and each `check` starts a fresh search. That is correct but slow for long incremental sessions.
- **Theory checks run only on full assignments.** There is no theory propagation, and the simplex is rebuilt for each check.
- **No restarts and no clause deletion.**
- **Unsat cores are deletion-based.** They take one solver call per assertion.
- **Only real variables and linear terms.** Nonlinear products, `ite` and `distinct` are rejected.
- **Not yet cross-checked against the SMT-LIB corpus.** Testing uses a randomized oracle (DNF plus Fourier-Motzkin elimination) and unit tests.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

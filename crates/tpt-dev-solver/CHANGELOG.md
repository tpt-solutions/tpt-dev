# Changelog

All notable changes to `tpt-dev-solver` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- QF_LRA solver core: CDCL SAT search over a general simplex with delta-rationals for strict bounds.
- Exact rational arithmetic on checked `i128`. Overflow returns `Unknown` instead of a wrong answer.
- Models for every `Sat` answer, read with `Solver::value`.
- Incremental `push` and `pop` that keep learned clauses and the simplex tableau.
- `Budget` limits on formula nodes, clauses and simplex pivots.
- `FixedArena`, a formula store with const-generic capacity and no heap allocation.
- Minimal unsat cores through `Solver::unsat_core`.
- Farkas certificates for conjunctions of linear literals, with `check_farkas` to verify them without the solver.
- DRUP-style unsat proofs for Boolean combinations of linear atoms, with `check_proof`.
- An SMT-LIB subset runner, `Session`, for `declare-fun`, `assert`, `check-sat`, `get-model`, `push`, `pop` and `exit`.
- A `bench` example that times random QF_LRA instances.
- Exhaustive equivalence tests for the Tseitin encoder, covering formulas of up to 16 CNF variables.

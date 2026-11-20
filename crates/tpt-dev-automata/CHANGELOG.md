# Changelog

All notable changes to `tpt-dev-automata` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Machine`, a finite state machine with states, event transitions, final states and optional linear-arithmetic guards.
- `Machine::analyze`, which finds reachable and unreachable states and the deadlocks among the reachable, non-final ones.
- Deadlock kinds `NoTransitions` and `GuardsIncomplete`. The second carries a witness valuation from `tpt-dev-solver`.
- A shortest event trace from the initial state for every deadlock.
- Re-exports of `Arena`, `Formula`, `Linear`, `Rational` and `Var` from `tpt-dev-solver`, so guards can be built without a direct dependency.
- `const_check`, which proves deadlock freedom in a `const` context for machines with box guards (conjunctions of `x <op> integer`), with `assert_deadlock_free` and `check_deadlock_free`.
- Size limits on the const check, with `Verdict::TooLarge` for machines above them.
- `trybuild` compile-fail tests for a machine with a missing transition and a machine with a guard gap, and a randomized test that the const check agrees with the run-time analysis.

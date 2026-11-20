# Changelog

All notable changes to `tpt-dev-contract` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `#[requires]` attribute, checked on function entry in debug builds.
- `#[ensures]` attribute, checked on return in debug builds unless it was proven at compile time.
- Compile-time proof of `ensures` clauses with `tpt-dev-solver` for bodies made of `let` statements and a final expression in linear integer arithmetic.
- Compile errors with an integer counterexample when the solver refutes an `ensures` clause.
- Several `requires` and `ensures` attributes on one function, in plain or path-style form.
- `old(x)` in `ensures` clauses, which names the value a parameter had on entry. The proof uses a snapshot variable and the run-time check copies the parameter before the body runs.
- A compile error for `old` in `requires`, for `old` on a name that is not a parameter, and for `old` with anything other than one name.
- `trybuild` compile-fail tests for refuted contracts, and tests for proven contracts.

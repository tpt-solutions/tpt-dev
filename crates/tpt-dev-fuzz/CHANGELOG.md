# Changelog

All notable changes to `tpt-dev-fuzz` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Fuzzer`, a coverage-guided mutation loop over fixed-length `i64` inputs, with a configurable iteration budget, value range and corpus cap.
- `Coverage` and `Outcome`, so a target closure can report edges and return `Pass` or `Crash`.
- `solver_seeds`, which asks `tpt-dev-solver` for integer inputs that satisfy a linear precondition, with `SeedError` for unsatisfiable, undecided and non-integral cases.
- `minimise`, which shrinks a crashing input toward zero. The fuzzer applies it to the crash it reports.
- A test that shows solver-seeded fuzzing finds a bug behind `a + b + c + d = 1234` that plain fuzzing misses in the same budget.

# Changelog

All notable changes to `tpt-dev-test` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- Deterministic property runner: `check` runs a property over generated cases, and `replay` regenerates any case from the run seed and case index.
- Greedy shrinking of failing inputs, bounded by `Config::max_shrinks`.
- Generators `IntRange`, `Bools`, `VecOf` and `Pair`, and the `Generator` trait for custom ones.
- A small seeded `Rng` with `Seed`, so runs do not depend on an external crate.
- `ConstrainedGen`, which generates assignments that satisfy a linear real constraint using `tpt-dev-solver`.
- Invariant inference: `infer` proposes linear bounds from samples, and `prove` checks a candidate against a precondition with the solver.
- Inductive invariant search for transition systems: `find_invariant` samples random walks, mines candidate bounds and filters them with solver-checked initiation and consecution.
- Outcomes `Proven`, `Violated` (with a concrete trace), `Vacuous` and `NoInvariant` from `find_invariant`.

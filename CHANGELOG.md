# Changelog

All notable changes to tpt-dev are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The project uses one shared version for all crates; see [VERSIONING.md](VERSIONING.md).

Minimum supported Rust version (MSRV) bumps are listed under their release, as [CONTRIBUTING.md](CONTRIBUTING.md) requires.

## [Unreleased]

Nothing is published yet. Version `0.0.0` is the workspace placeholder.

### Added
- `tpt-dev` facade crate: one dependency, with feature groups `verify`, `contract`, `rt`, `ffi`, `security`, `edge`, `lang`, `bench`, `tools`, `full`, and `std` forwarding.
- `tpt-dev-cli` (`cargo tpt`): `check`, `lint`, `dep`, `headers`, `doc` and `bench-gate` subcommands.
- `tpt-dev-dep`: `--json` output; licences read from `cargo metadata` when no `licenses.txt` is given; parenthesised licence expressions.
- `tpt-dev-bench`: `bench_suite` example, which writes results in the baseline format, and a committed sample baseline for the CI gate.
- `tpt-dev-bench`: the critical t value now comes from the Welch degrees of freedom (`t_critical_95`). `--critical-t` overrides it.
- Workflows: `project-checks` (runs `cargo tpt check`) and `docs` (publishes rustdoc to GitHub Pages).

### Documentation
- `docs/api-review.md`: first-pass 1.0 stability review for every crate.
- `VERSIONING.md`: the release procedure, its preconditions, and the first-release (`0.1.0`) blocker.

### Changed
- `tpt-dev-bench`: `Policy::critical_t` is now `Option<f64>`; `None` (the default) uses the degrees-of-freedom value. `Comparison` has a `df` field.
- `tpt-dev-bench` gate workflow example: the placeholders are filled in, and `--critical-t` is no longer passed.

### Removed
- Stray trybuild `wip/` output directories in `automata`, `codec`, `dsl-macros`, `lock` and `mock`.

### Fixed
- `tpt-dev-ffi` tests: every `unsafe` block now has a `SAFETY` comment.

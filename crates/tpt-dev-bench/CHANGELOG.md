# Changelog

All notable changes to `tpt-dev-bench` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- Summary statistics (`summarise`, `Summary`) and Tukey outlier trimming (`trim_outliers`).
- Welch's t-test comparison (`compare`, `Comparison`) with Welch degrees of freedom and a `t_critical_95` lookup.
- Regression gate (`judge`, `judge_entry`, `Policy`, `Verdict`) that fails only on a large, trusted slowdown.
- Plain-text baseline format (`Baseline`, `Entry`, `ParseError`).
- `no_std` cycle-counter timing (`CycleSource`, `measure_with`, `ManualCycles`) for bare-metal targets.
- `std` feature with `harness::measure`, based on `Instant`.
- `gate` example: a CI command that compares a results file with a baseline and exits 1 on a trusted regression.
- `bench_suite` example, a template benchmark runner for the gate.
- `examples/ci/bench-gate.yml`: a pull request workflow template, not yet run in CI.

### Changed
- `Policy::critical_t` is an `Option<f64>`. The default (`None`) uses the 95% t critical value for the comparison's degrees of freedom instead of a fixed 2.0.

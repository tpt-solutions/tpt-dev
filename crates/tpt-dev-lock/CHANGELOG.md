# Changelog

All notable changes to `tpt-dev-lock` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Mutex<L, T>`: a spin mutex whose lock order is checked at compile time through lock levels (`L1` to `L8`) and `Token`s.
- Compile-fail tests (`trybuild`) for reversed lock order and same-level nesting.
- `TripleBuffer<T>`: a wait-free single-writer, single-reader register with `Writer` and `Reader` handles.
- `Spsc<T, N>`: a lock-free, fixed-capacity single-producer single-consumer queue with `Producer` and `Consumer` handles.
- Loom models and multi-thread tests for the primitives.
- Optional `std` feature.

### Changed
- `loom` is now taken from the workspace dependency table.

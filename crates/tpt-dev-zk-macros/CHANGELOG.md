# Changelog

All notable changes to `tpt-dev-zk-macros` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- The `#[circuit]` attribute, which keeps an `i64` function and generates a `Ring`-generic `<name>_circuit` with the same body.
- Support for integer literals, parameters, `let` bindings, parentheses, `+`, `-`, `*`, unary `-` and nested blocks.
- Compile errors that name the unsupported construct, including generics, methods, non-`i64` types, patterns, `let`-`else`, comparisons, division, `if` and calls.
- A README for the crate.

### Changed
- `syn`, `quote` and `proc-macro2` now come from the workspace dependency table.

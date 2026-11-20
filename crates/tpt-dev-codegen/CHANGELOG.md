# Changelog

All notable changes to `tpt-dev-codegen` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Writer` that builds Rust source with indentation and balanced braces from the structure of its calls.
- `Writer::constant` and `Writer::str_constant`, with checked constant names and escaped string values.
- `is_ident` and `string_literal` helpers, and an `Error` type that `Writer::finish` returns for the first bad name.
- `build` module (feature `std`) with `write_if_changed`, `rerun_if_changed` and `rerun_if_env_changed` for `build.rs`.
- `compare` example that checks `Writer` output against hand-written `format!` output.
- `no_std` with `alloc` for the writer.

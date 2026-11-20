# Changelog

All notable changes to `tpt-dev-headers` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `generate` and `Options`, which turn the public C-ABI items of a Rust source file into a C header with an include guard.
- Output for `extern "C"` functions, `#[repr(C)]` structs, integer-backed enums, opaque structs, type aliases (including function-pointer types) and integer constants.
- Doxygen comments from Rust `///` documentation, on items and on struct fields.
- Type mapping for the fixed-width integers, `usize`, `isize`, floats, `bool`, the `std::ffi` C types, raw pointers, references and `extern "C" fn` pointers.
- An `Error` type that names the item holding an unsupported type or construct.
- A `tpt-dev-headers` command: `tpt-dev-headers <SOURCE.rs> <GUARD> <OUTPUT.h>`, with exit codes 0, 1 and 2.
- `build::write_header` and `build::rerun_if_changed` for `build.rs`, with `BuildError`. The output file is rewritten only when its contents change.
- A golden-file test against a frozen header, and a C client that CI compiles against a generated header.

### Changed
- `syn` is now taken from the workspace dependencies.

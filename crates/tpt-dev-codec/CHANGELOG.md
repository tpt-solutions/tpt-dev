# Changelog

All notable changes to `tpt-dev-codec` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Field`, `Endian` and `Layout<B>` for fixed-size binary layouts built in a `const`.
- Compile-time validation of overlaps, out-of-range fields, widths outside 1 to 64 bits, duplicate or empty names and unaligned big-endian fields.
- `Layout::encode` and `Layout::decode` on caller buffers, with no allocation.
- `CodecError` for short buffers, wrong value counts and values too wide for their field.
- `Layout::with_version` and `Layout::extends` to check schema evolution in a const assertion.
- Compile-fail tests and property-based round-trip tests.
- `no_std` without `alloc`, and an optional `std` feature.

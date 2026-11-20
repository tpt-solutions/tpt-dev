# Changelog

All notable changes to `tpt-dev-zk` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Circuit` and `Wire`, which record R1CS constraints and the witness while ordinary Rust arithmetic runs.
- The `Ring` trait, so one generic function runs natively on `i64` and as a circuit on `Wire`.
- Private and public inputs, public outputs, equality assertions, and `Circuit::check` to find the first violated constraint.
- Export to the iden3 binary `.r1cs` format with `Circuit::to_r1cs`, and the witness in wire order with `Circuit::witness`.
- The `#[circuit]` attribute, re-exported from `tpt-dev-zk-macros`, which generates a `Ring`-generic `<name>_circuit` from an `i64` function.
- Compile-fail tests for unsupported syntax and unknown names in `#[circuit]` functions.
- An `export_r1cs` example that writes a sample circuit and witness for checking with snarkjs.

### Changed
- The field is now the 254-bit BN254 scalar field, with Montgomery arithmetic on four 64-bit limbs, instead of the earlier 2^61 - 1 field. The `.r1cs` export uses the BN254 prime and 32-byte coefficients.
- Added `Circuit::to_wtns`, which writes the `.wtns` witness format. The export was validated with snarkjs 0.7.4 (`snarkjs r1cs info` and `snarkjs wtns check`).

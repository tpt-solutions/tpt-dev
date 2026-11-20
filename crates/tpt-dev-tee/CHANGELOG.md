# Changelog

All notable changes to `tpt-dev-tee` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- The `SecureMemory`, `Attestation` and `SecureCall` traits, with `World`, `Report`, `Measurement`, `DIGEST_LEN` and `TeeError`.
- `mock::MockTee`, a host-test backend with fixed secure memory and a service table. It denies the normal world and checks bounds.
- `mock::measure`, which computes a measurement of a code image with the mock's digest function.
- Mock attestation: `quote` and `verify` reject a changed nonce, measurement, tag or key. The tag is not a cryptographic MAC.
- `trustzone::Partition` and `Region`, a Cortex-M secure and non-secure address model that defaults uncovered addresses to secure and refuses overlapping or empty regions.
- `trustzone::Gate`, which enters the secure world only at registered entry points and does not nest.
- The crate is `no_std`, with an optional `std` feature. It has no hardware backend, and has not been independently audited.

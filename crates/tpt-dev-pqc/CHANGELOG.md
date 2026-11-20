# Changelog

All notable changes to `tpt-dev-pqc` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- ML-KEM (FIPS 203) for 512, 768 and 1024, with seeded and `Rng`-driven key generation and encapsulation, and implicit rejection in decapsulation.
- ML-DSA (FIPS 204) for 44, 65 and 87: key generation, deterministic and hedged signing, and verification with a context string, in pure mode only.
- Checked key, ciphertext and signature parsing with length and modulus errors (`KemError`, `DsaError`).
- A Keccak sponge (SHA3 and SHAKE) and number-theoretic transforms, tested against reference values.
- NIST known-answer tests: the first three records per set are committed, and the full files (3,000 ML-KEM and 300 ML-DSA records) pass when supplied through `TPT_KAT_DIR`.
- `timing` example: a dudect-style leakage check and host benchmarks.

### Security
- ML-DSA signing runs a fixed budget of 64 attempts with masked selection, so the retry count no longer shows in host timing (|t| fell from about 56 to at most 0.43). Signatures are unchanged, and signing is about 9 to 15 times slower. Not measured on Cortex-M.
- This crate has not been independently audited.

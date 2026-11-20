# Changelog

All notable changes to `tpt-dev-mock` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `#[mock]` attribute (re-exported from `tpt-dev-mock-macros`) that generates a `Mock<Trait>` implementing the trait.
- `Recorder` per method, with `expect()`, `calls()` and `verify()`.
- `ExpectBuilder` with `with` (argument matcher), `times`, `returns` and `returns_with`.
- Unexpected calls panic with the method name, and `verify` panics when a `times` count is not met.
- Compile errors for generic traits, generic or async methods, associated constants and methods without a `&self` or `&mut self` receiver, covered by compile-fail tests.
- `no_std` with `alloc`, and an optional `std` feature.
- Associated types: `#[mock(Item = u32)]` binds each `type Item;`, and `Self::Item` in method signatures is replaced by the bound type. Unbound or unknown bindings are compile errors.
- `examples/compile_bench.rs`: compile-time benchmark of `#[mock]` against mockall's `#[automock]`, with mockall as a dev-dependency.

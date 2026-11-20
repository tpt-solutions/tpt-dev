# Changelog

All notable changes to `tpt-dev-mock-macros` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `#[mock]` proc-macro attribute that generates a mock struct with one recorder field per trait method, `new`, `verify` and a `Default` impl.
- Clear compile errors, spanned on the offending item, for unsupported trait shapes.
- Dependencies on `syn`, `quote` and `proc-macro2` shared through workspace dependencies.

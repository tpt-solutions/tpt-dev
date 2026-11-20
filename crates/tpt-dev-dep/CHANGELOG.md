# Changelog

All notable changes to `tpt-dev-dep` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Cargo.lock` parser (`parse_lock`) producing a dependency `Graph`.
- Cycle detection with Tarjan's algorithm.
- License policy (`Policy`, `license_allowed`) for SPDX-style expressions with `OR`, `AND` and `WITH`.
- Transitive dependency counts per package, reported largest first (`analyse`, `Report`).
- `cargo metadata --format-version 1` input (`parse_metadata`) using a strict built-in JSON parser (`parse_json`, `Value`, `MAX_DEPTH`).
- Dependency kinds (`DepKind`) and a `Scope` that follows dev and build edges only when asked.
- `tpt-dev-dep` command with `--metadata`, `--allow`, `--include-dev` and `--include-build`, exiting 1 on a cycle or license violation and 2 on a usage error.
- A test fixture with a `Cargo.lock` and `cargo metadata` output.

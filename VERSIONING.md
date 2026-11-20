# Versioning policy

tpt-dev follows [Semantic Versioning 2.0.0](https://semver.org/), with the pre-1.0 rules below.

## Before 1.0

- **Breaking changes are allowed in minor releases.** A release `0.x.0` may change public APIs. Each one is listed in the release notes.
- **Patch releases do not break.** A `0.x.y` release fixes bugs and may add items, but it must not change or remove public API.
- **Documentation and dependencies.** A change to documented behaviour is a breaking change if callers rely on it, even when the signature stays the same.

## Shared workspace version

All 30 crates share one version number, set in the workspace `Cargo.toml`, and are released together with `cargo-release` (see `release.toml`). Crates that depend on each other use that same version in their path dependencies. A crate may only depend on a version it was released with.

The current version is `0.0.0`. Nothing is published. The first release, `0.1.0`, is part of Phase 7 and has not been made.

## From 1.0

- **Major:** any breaking change to a public item, including its type, its error variants, and any behaviour documented as guaranteed.
- **Minor:** added public items, and added variants where the enum is marked non-exhaustive.
- **Patch:** fixes that change no public API.

Crates that reach 1.0 need a stability review first. A first-pass review, with per-crate breaking-change candidates, is in [docs/api-review.md](docs/api-review.md). Its items still need to be acted on before 1.0.

## Compatibility

- **Toolchain.** The workspace declares `rust-version` in its `Cargo.toml`. Raising it is a minor change before 1.0 and a major change after 1.0.
- **`no_std`.** Every crate builds for `thumbv7em-none-eabihf`, except the host-only crates excluded in CI (see `.github/workflows/ci.yml`). Dropping the bare-metal build for a crate is a breaking change.
- **Features.** The `std` feature only adds functionality. Removing it is a breaking change.

## Releasing

Releases use `cargo-release`, configured in `release.toml`. It bumps the shared version, commits, creates one tag `v<version>`, publishes the crates in dependency order (cargo-release works this out from the path dependencies), and pushes. Nothing is published by a dry run.

### Preconditions

1. **Committed, clean tree on `master`.** `cargo-release` refuses to run with uncommitted changes.
2. **Local checks pass:** `cargo tpt check` (lint, dependency and licence policy, `fmt`, `clippy`) and `cargo deny check`.
3. **CI is green on that commit**, including the `loom` and `c-headers` jobs in `.github/workflows/ci.yml`.
4. **CHANGELOG is ready.** Move the `[Unreleased]` entries under `## [X.Y.Z] - YYYY-MM-DD`. List any MSRV change, and every breaking change in a `0.x` minor release.
5. **The level matches the change.** Use `patch` only for fixes with no public API change. Before 1.0, use `minor` for anything else (see the rules above).

### Steps

1. **Dry run.** `cargo release <patch|minor> --workspace` (a dry run by default). Read the plan: it must name the expected version and report no errors. The same dry run runs from `.github/workflows/release.yml` with `execute` left false.
2. **Execute.** Either run `cargo release <level> --workspace --execute` locally, or start `.github/workflows/release.yml` with `execute` set to true. The workflow needs the `CARGO_REGISTRY_TOKEN` secret, and it passes `--no-confirm`.
3. **Check.** Confirm each crate appears on crates.io, that docs.rs builds each one (see the Phase 7 item in `todo.md`), and that the `v<version>` tag is on `origin`.

Do not add `--no-verify` to an execute run. Verification runs `cargo publish --dry-run` for each crate, and that is the check that catches a broken package.

### First release (0.1.0)

The workspace is at `0.0.0`, and the first release is `0.1.0`. Use `minor` for it, not `patch`: `patch` from `0.0.0` produces `0.0.1`.

**Blocked as of 2026-11-20.** A dry run of `cargo release minor --workspace --no-verify` fails with 48 errors of the form "`<crate> 0.1.0` depends on unpublished workspace package `<dep> 0.1.0`". The errors show that the dependents' version requirements do not match `0.1.0`. The cause is not yet confirmed; the path dependencies carry `version = "0.0.0"`, which is one thing to check. Fix this, and re-run the dry run on a committed tree, before the first execute. The `patch` dry run plans `v0.0.1` and does not show this error.

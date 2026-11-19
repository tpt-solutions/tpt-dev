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

Crates that reach 1.0 need a stability review first. That review is still to be done (see `todo.md`, Phase 7).

## Compatibility

- **Toolchain.** The workspace declares `rust-version` in its `Cargo.toml`. Raising it is a minor change before 1.0 and a major change after 1.0.
- **`no_std`.** Every crate builds for `thumbv7em-none-eabihf`, except the host-only crates excluded in CI (see `.github/workflows/ci.yml`). Dropping the bare-metal build for a crate is a breaking change.
- **Features.** The `std` feature only adds functionality. Removing it is a breaking change.

## Releasing

Releases are cut by hand with `cargo release`, following the publish order in `todo.md`. A patch release uses `cargo release patch --workspace --execute`. The release workflow in `.github/workflows/release.yml` runs as a dry run unless it is started with publishing enabled.

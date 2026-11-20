# Contributing to tpt-dev

tpt-dev has a single maintainer, run on a benevolent-dictator model. **Contributions are issues only.** Pull requests are not accepted and will be closed without review. The maintainer makes the final call on scope, API design and releases.

## How to contribute

Open an issue for any of the following:

- **Bug reports:** include a minimal reproduction, the crate and version, the toolchain, and the target (host or `thumbv7em-none-eabihf`).
- **Feature requests and API proposals:** describe the problem first, then the proposed signatures. Changes must fit a crate in the todo list. New crates, or changes that move work between crates, need discussion here before anything else.
- **Questions and documentation gaps.**

Security issues should be reported privately to the maintainer, not in a public issue.

## Project constraints

Proposals should respect these, which the maintainer enforces on all changes:

- **Dependencies:** runtime crates stay zero-dependency. Proc-macro crates may use `syn`, `quote` and `proc-macro2`. Dependencies must pass `cargo deny check licenses`.
- **No unsafe without a reason:** `unsafe_code` is denied at workspace level.
- **no_std:** every crate builds for `thumbv7em-none-eabihf`. `std` support goes behind the `std` feature.

## Licensing

Code or text you include in an issue may be used in the project under MIT OR Apache-2.0, without additional terms.

## MSRV policy

- The minimum supported Rust version (MSRV) is the latest stable release minus two releases.
- The MSRV is recorded as `rust-version` in the root `Cargo.toml`.
- MSRV bumps happen only in minor versions and are listed in the changelog.
- The toolchain used for development is pinned in `rust-toolchain.toml`.

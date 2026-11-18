# Contributing to tpt-dev

tpt-dev has a single maintainer, run on a benevolent-dictator model. Contributions are welcome, but the maintainer makes the final call on scope, API design and releases.

## Boundaries

- **Scope:** changes must fit a crate in the todo list. New crates, or changes that move work between crates, need prior discussion in an issue.
- **Design first:** for a new public API, open an issue with the proposed signatures before writing the implementation.
- **Dependencies:** runtime crates stay zero-dependency. Proc-macro crates may use `syn`, `quote` and `proc-macro2`. Each new dependency needs a license that passes `cargo deny check licenses`.
- **No unsafe without a reason:** `unsafe_code` is denied at workspace level. Where `unsafe` is required, each block needs a `// SAFETY:` comment (enforced by `undocumented_unsafe_blocks`).
- **no_std:** every crate must build for `thumbv7em-none-eabihf`. `std` support goes behind the `std` feature.
- **Docs:** every public item needs a doc comment. Public functions need an example or an `include_str!` of the crate README.

## Before opening a pull request

```sh
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo test --workspace --target thumbv7em-none-eabihf --no-run
cargo deny check licenses
```

## Licensing of contributions

Unless you state otherwise, any contribution you submit is dual-licensed under MIT OR Apache-2.0, without additional terms.

## MSRV policy

- The minimum supported Rust version (MSRV) is the latest stable release minus two releases.
- The MSRV is recorded as `rust-version` in the root `Cargo.toml`.
- MSRV bumps happen only in minor versions and are listed in the changelog.
- The toolchain used for development is pinned in `rust-toolchain.toml`.

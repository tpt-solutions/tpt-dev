# Getting started

This guide takes you from a clean checkout to a passing check, then to your first dependency on a tpt-dev crate. Nothing here needs a publish, because tpt-dev is not published yet.

## 1. Install the toolchain

tpt-dev pins its toolchain in [rust-toolchain.toml](../rust-toolchain.toml), which includes the `thumbv7em-none-eabihf` target. With [rustup](https://rustup.rs), the pinned toolchain installs itself the first time you run cargo in the repository.

To build for bare metal, install the target if rustup has not already:

```sh
rustup target add thumbv7em-none-eabihf
```

Optional tools:

- `cargo-deny`, for the licence check: `cargo install --locked cargo-deny`
- A C compiler (`gcc`), for the C-header CI job only.

The [devcontainer](../.devcontainer/devcontainer.json) installs all of these for you.

## 2. Check the workspace

```sh
git clone https://github.com/tpt-solutions/tpt-dev
cd tpt-dev
cargo run -q -p tpt-dev-cli --bin cargo-tpt -- check
```

`check` runs formatting, clippy, the project lint rules and the dependency policy. It prints a summary and exits 0 if everything passed. Add `--full` to also run the tests and `cargo deny`.

To run the tests:

```sh
cargo test --workspace
```

## 3. Try an example

The facade crate has three cross-crate examples. Start with the one for your area:

```sh
cargo run -p tpt-dev --example verified_contract --features contract,verify
```

See [crates/tpt-dev/README.md](../crates/tpt-dev/README.md) for all of them.

## 4. Depend on a crate

Pick the group that covers what you need, using [which-crate.md](which-crate.md) to find it. Add the facade by path while tpt-dev is unpublished:

```toml
[dependencies]
tpt-dev = { path = "../tpt-dev/crates/tpt-dev", features = ["verify"] }
```

Then use the module it exposes:

```rust
use tpt_dev::solver::Session;

fn main() {
    let mut session = Session::new();
    let answer = session
        .run("(declare-const x Real)(assert (> x 1))(assert (< x 0))(check-sat)")
        .unwrap();
    print!("{answer}");
}
```

For a bare-metal project, depend on the facade with no features, or on single crates with `default-features = false`, and build for `thumbv7em-none-eabihf`.

## 5. Where to go next

- [feature-matrix.md](feature-matrix.md): which crate is `no_std`, which needs `std`, which is host-only.
- [which-crate.md](which-crate.md): problems mapped to crates.
- [CONTRIBUTING.md](../CONTRIBUTING.md): how to propose a change. Contributions are issues only.
- [todo.md](../todo.md): open work and the status of each crate.

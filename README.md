# tpt-dev

Foundational, pure-Rust developer tooling: 30 small crates covering formal verification, hard real-time concurrency, edge-AI memory management, post-quantum cryptography and embedded tooling.

Every crate is `no_std` first, with `std` behind an optional feature. Hot paths are designed to avoid heap allocation.

**Status:** pre-release. The workspace is scaffolded; crate implementations are in progress and nothing is published yet.

## Crates

| Group | Crates |
|---|---|
| Formal verification | `tpt-dev-solver`, `tpt-dev-contract`, `tpt-dev-fuzz`, `tpt-dev-test` |
| FFI and AST | `tpt-dev-headers`, `tpt-dev-ffi`, `tpt-dev-ast` |
| Real-time and concurrency | `tpt-dev-async`, `tpt-dev-trace`, `tpt-dev-lock`, `tpt-dev-mem` |
| Performance and codegen | `tpt-dev-profiler`, `tpt-dev-bench`, `tpt-dev-codec`, `tpt-dev-dsl`, `tpt-dev-codegen` |
| Developer experience | `tpt-dev-doc`, `tpt-dev-lint`, `tpt-dev-dep`, `tpt-dev-mock` |
| AI and edge | `tpt-dev-paged`, `tpt-dev-dispatch` |
| Security | `tpt-dev-pqc`, `tpt-dev-zk` |
| Debugging and logic | `tpt-dev-replay`, `tpt-dev-automata` |
| Wasm and hardware | `tpt-dev-compose`, `tpt-dev-sim` |
| Enclaves and telemetry | `tpt-dev-tee`, `tpt-dev-sink` |

## Layout

```
crates/tpt-dev-*/   one directory per crate
deny.toml           license policy (cargo-deny)
rust-toolchain.toml pinned toolchain, with thumbv7em-none-eabihf target
```

## Versioning

All crates share one version. Before 1.0, minor releases may break the API, and patch releases may not. See [VERSIONING.md](VERSIONING.md). Crate status and open work are tracked in [todo.md](todo.md).

## Building and checking

```sh
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo test --workspace --target thumbv7em-none-eabihf --no-run
cargo deny check licenses
```

## License

Dual-licensed under either of

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option. Dependencies must offer an MIT-compatible license. Apache-only, GPL and AGPL dependencies are rejected. See [CONTRIBUTING.md](CONTRIBUTING.md) and [deny.toml](deny.toml).

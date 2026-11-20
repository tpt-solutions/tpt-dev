# tpt-dev

Foundational, pure-Rust developer tooling: 33 crates covering formal verification, hard real-time concurrency, edge-AI memory management, post-quantum cryptography and embedded tooling. Four of them are proc-macro companions. Two more are the `tpt-dev` facade and the `cargo tpt` command.

Every crate is `no_std` first, with `std` behind an optional feature. Hot paths are designed to avoid heap allocation.

**Status:** pre-release. Nothing is published yet. See [todo.md](todo.md) for open work.

## Start here

- **New to the project:** [docs/getting-started.md](docs/getting-started.md) takes you from a clean checkout to a passing check and a first dependency.
- **Not sure which crate you need:** [docs/which-crate.md](docs/which-crate.md) maps problems to crates.
- **Depending on one or more crates:** add the `tpt-dev` facade with feature groups, or depend on single crates directly. The [feature and `no_std` matrix](docs/feature-matrix.md) lists what each crate needs.

## Crates

The status column uses three levels. **alpha** means implemented and tested, but the API may change. **wip** means a known part is partial or missing, and the README of the crate lists the limits.

| Group | Crate | Status | Purpose |
|---|---|---|---|
| Facade | `tpt-dev` | alpha | One dependency; feature groups re-export the crates below |
| Tooling | `tpt-dev-cli` | alpha | `cargo tpt`: the project checks in one command |
| Formal verification | `tpt-dev-solver` | alpha | Pure-Rust QF_LRA SMT solver |
| | `tpt-dev-contract` | alpha | `#[requires]` / `#[ensures]`, proved where decidable |
| | `tpt-dev-fuzz` | alpha | Solver-guided structural fuzzer |
| | `tpt-dev-test` | alpha | Property testing with invariant generation |
| | `tpt-dev-automata` | alpha | State machine verifier: reachability and deadlock |
| FFI and AST | `tpt-dev-headers` | alpha | C header generator from Rust `extern "C"` items |
| | `tpt-dev-ffi` | alpha | Typestate sessions for stateful C libraries |
| | `tpt-dev-ast` | alpha | Zero-copy streaming AST parser framework |
| Real-time and concurrency | `tpt-dev-async` | alpha | Hard real-time no_std async executor |
| | `tpt-dev-trace` | alpha | Zero-allocation distributed tracing |
| | `tpt-dev-lock` | alpha | Deadlock-free sync primitives |
| | `tpt-dev-mem` | alpha | Safe MMIO register framework |
| Performance and codegen | `tpt-dev-profiler` | alpha | Sampling profiler with bounded per-sample cost |
| | `tpt-dev-bench` | alpha | Statistical benchmarking and CI regression gate |
| | `tpt-dev-codec` | alpha | Compile-time validated bit-level serialization |
| | `tpt-dev-dsl` | alpha | PEG grammar framework and embedded DSL macro |
| | `tpt-dev-codegen` | alpha | Build-time Rust source generation |
| Developer experience | `tpt-dev-doc` | alpha | Runnable documentation, static site output |
| | `tpt-dev-lint` | alpha | Project-specific lint rules, no compiler plugins |
| | `tpt-dev-dep` | alpha | Dependency graph: cycles, licences, bloat |
| | `tpt-dev-mock` | alpha | Compile-time mocking for traits |
| AI and edge | `tpt-dev-paged` | alpha | Paged memory manager for LLM KV-caches |
| | `tpt-dev-dispatch` | alpha | Heterogeneous compute task scheduler |
| Security | `tpt-dev-pqc` | wip | ML-KEM and ML-DSA; constant-time work is partial |
| | `tpt-dev-zk` | alpha | Rust-to-R1CS transpiler with circom export |
| | `tpt-dev-tee` | wip | TEE abstraction; the TrustZone backend is a model only |
| Debugging and logic | `tpt-dev-replay` | alpha | Deterministic execution recorder and replay |
| Wasm and hardware | `tpt-dev-compose` | wip | Wasm component parser and linker; plans, does not execute |
| | `tpt-dev-sim` | alpha | Cycle-accurate hardware simulator |
| Enclaves and telemetry | `tpt-dev-sink` | alpha | Zero-allocation edge telemetry router |

Companion proc-macro crates, which the crates above re-export or depend on: `tpt-dev-contract`, `tpt-dev-dsl-macros`, `tpt-dev-mock-macros`, `tpt-dev-zk-macros`.

## Layout

```
crates/tpt-dev/           facade: feature groups over the crates
crates/tpt-dev-cli/       cargo-tpt: project checks
crates/tpt-dev-*/         one directory per crate
docs/                     guides
deny.toml                 license policy (cargo-deny)
rust-toolchain.toml       pinned toolchain, with thumbv7em-none-eabihf target
```

## Versioning

All crates share one version. Before 1.0, minor releases may break the API, and patch releases may not. See [VERSIONING.md](VERSIONING.md). Changes are recorded in [CHANGELOG.md](CHANGELOG.md).

## Building and checking

```sh
cargo tpt check            # fmt, clippy, project lint rules, dependency policy
cargo tpt check --full     # also runs the tests and cargo-deny
cargo test --workspace
cargo build --workspace --exclude tpt-dev-contract --exclude tpt-dev-mock-macros \
  --exclude tpt-dev-zk-macros --exclude tpt-dev-dsl-macros --exclude tpt-dev-dep \
  --exclude tpt-dev-headers --exclude tpt-dev-lint --exclude tpt-dev-cli \
  --target thumbv7em-none-eabihf
```

`cargo tpt` runs the same checks as CI. The full list of subcommands is in [crates/tpt-dev-cli](crates/tpt-dev-cli/README.md).

## License

Dual-licensed under either of

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option. Dependencies must offer an MIT-compatible license. Apache-only, GPL and AGPL dependencies are rejected. See [CONTRIBUTING.md](CONTRIBUTING.md) and [deny.toml](deny.toml).

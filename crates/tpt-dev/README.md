# tpt-dev

One dependency for the tpt-dev crates. Enable feature groups; each crate in a group appears as a module.

```toml
[dependencies]
tpt-dev = { version = "0.0.0", features = ["verify", "rt"] }
```

```rust
use tpt_dev::solver::Session;
```

The feature groups, and which crate each one enables, are in the module documentation of [`src/lib.rs`](src/lib.rs) and in [docs/feature-matrix.md](../../docs/feature-matrix.md). The default build enables nothing and is `no_std`.

## Examples

Each example needs the feature groups it uses:

```sh
cargo run -p tpt-dev --example verified_contract --features contract,verify
cargo run -p tpt-dev --example fsm_ffi --features verify,ffi,tools
cargo run -p tpt-dev --example replay_trace --features rt
```

- `verified_contract`: `#[requires]` and `#[ensures]` on functions, and a solver check of a property.
- `fsm_ffi`: one state machine checked for deadlock, used to drive a typestate FFI session, and a C header generated from the library source.
- `replay_trace`: record an async run, replay it, check the results agree, and count trace records.

## Not yet included

Some crates are not in any group yet: `tpt-dev-dsl-macros`, `tpt-dev-mock-macros` and `tpt-dev-zk-macros` are companions used by their parent crates. The proc-macro `contract` attributes are host-only.

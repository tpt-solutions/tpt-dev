# Feature and `no_std` matrix

Every library crate is `#![no_std]`. Its `std` feature is optional and off by default. Proc-macro and host-tool crates are built for the host only.

The facade `tpt-dev` enables crates through feature groups. Use the group name for the group, or depend on a crate directly if you need only one.

| Crate | Kind | `std` feature | Facade group | In bare-metal CI |
|---|---|---|---|---|
| `tpt-dev` | facade | forwards `std` | (this crate) | yes |
| `tpt-dev-ast` | library | yes | `lang` | yes |
| `tpt-dev-async` | library | yes | `rt` | yes |
| `tpt-dev-automata` | library | yes | `verify` | yes |
| `tpt-dev-bench` | library | yes | `bench` | yes |
| `tpt-dev-codec` | library | yes | `ffi` | yes |
| `tpt-dev-codegen` | library | yes | `lang` | yes |
| `tpt-dev-compose` | library | yes | `ffi` | yes |
| `tpt-dev-contract` | proc-macro | no | `contract` | **no** (host only) |
| `tpt-dev-dep` | host tool | no | `tools` | **no** (host only) |
| `tpt-dev-dispatch` | library | yes | `edge` | yes |
| `tpt-dev-doc` | library and host bin | yes | `tools` | yes |
| `tpt-dev-dsl` | library | yes | `lang` | yes |
| `tpt-dev-dsl-macros` | proc-macro | no | (via `tpt-dev-dsl`, feature `macros`) | **no** (host only) |
| `tpt-dev-ffi` | library | yes | `ffi` | yes |
| `tpt-dev-fuzz` | library | yes | `verify` | yes |
| `tpt-dev-headers` | host tool | no | `tools` | **no** (host only) |
| `tpt-dev-lint` | host tool | no | `tools` | **no** (host only) |
| `tpt-dev-lock` | library | yes | `rt` | yes |
| `tpt-dev-mem` | library | yes | `edge` | yes |
| `tpt-dev-mock` | library | yes | `verify` | yes |
| `tpt-dev-mock-macros` | proc-macro | no | (via `tpt-dev-mock`) | **no** (host only) |
| `tpt-dev-paged` | library | yes | `edge` | yes |
| `tpt-dev-pqc` | library | yes | `security` | yes |
| `tpt-dev-profiler` | library | yes | `edge` | yes |
| `tpt-dev-replay` | library | yes | `rt` | yes |
| `tpt-dev-sim` | library | yes | `edge` | yes |
| `tpt-dev-sink` | library | yes | `rt` | yes |
| `tpt-dev-solver` | library | yes | `verify` | yes |
| `tpt-dev-tee` | library | yes | `security` | yes |
| `tpt-dev-test` | library | yes | `verify` | yes |
| `tpt-dev-trace` | library | yes | `rt` | yes |
| `tpt-dev-zk` | library | yes | `security` | yes |
| `tpt-dev-zk-macros` | proc-macro | no | (via `tpt-dev-zk`) | **no** (host only) |
| `tpt-dev-cli` | host tool (`cargo-tpt`) | no | none | **no** (host only) |

"In bare-metal CI" means the crate is built for `thumbv7em-none-eabihf` by the `no-std` job in `.github/workflows/ci.yml`.

## Facade feature groups

| Group | Enables |
|---|---|
| `verify` | `solver`, `automata`, `test`, `fuzz`, `mock` |
| `contract` | `contract` (host only; implies `verify`) |
| `rt` | `async`, `trace`, `lock`, `sink`, `replay` |
| `ffi` | `ffi`, `codec`, `compose` |
| `security` | `pqc`, `tee`, `zk` |
| `edge` | `mem`, `paged`, `dispatch`, `sim`, `profiler` |
| `lang` | `ast`, `dsl`, `codegen` |
| `bench` | `bench` |
| `tools` | `headers`, `lint`, `dep`, `doc` (host only, uses `std`) |
| `full` | every no_std library group above, except `contract` and `tools` |
| `std` | forwards `std` to every enabled crate that has one |

The default build enables nothing. It builds for `thumbv7em-none-eabihf` with no dependencies.

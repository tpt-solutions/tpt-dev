# Which crate do I need?

Find the problem you have, then use the crate in that row. The [crate table](../README.md#crates) gives each crate's status.

## Verification and correctness

| I want to... | Use |
|---|---|
| Check that a set of linear real constraints is satisfiable, and get a model | `tpt-dev-solver` |
| Write `#[requires]` / `#[ensures]` on functions and prove them where possible | `tpt-dev-contract` (with `tpt-dev-solver`) |
| Check that a state machine cannot deadlock, and find the shortest path to a deadlock | `tpt-dev-automata` |
| Generate random test inputs that satisfy a linear precondition | `tpt-dev-fuzz` (uses `tpt-dev-solver`) |
| Write property tests, and infer loop-style invariants | `tpt-dev-test` |
| Mock a trait in tests, with no boilerplate | `tpt-dev-mock` |
| Prove an arithmetic circuit computes what the Rust code does | `tpt-dev-zk` |

## Real-time and concurrency

| I want to... | Use |
|---|---|
| Run async tasks on a bare-metal target with a bounded poll latency | `tpt-dev-async` |
| Share data between tasks without deadlock | `tpt-dev-lock` |
| Record spans and events with no heap allocation | `tpt-dev-trace` |
| Send telemetry through lock-free batching and routing rules | `tpt-dev-sink` |
| Record a run and replay it bit-for-bit | `tpt-dev-replay` |
| Read and write hardware registers safely | `tpt-dev-mem` |
| Measure where time goes, with bounded cost per sample | `tpt-dev-profiler` |

## FFI and interop

| I want to... | Use |
|---|---|
| Generate a C header from Rust `extern "C"` items | `tpt-dev-headers` |
| Wrap a stateful C library so that wrong call orders do not compile | `tpt-dev-ffi` |
| Serialise bit-fields and fixed layouts, checked at compile time | `tpt-dev-codec` |
| Link WebAssembly components (parse, check types, plan bindings) | `tpt-dev-compose` (plans only; does not run code) |

## Parsing, languages and codegen

| I want to... | Use |
|---|---|
| Parse a large input in a fixed stack buffer, streaming | `tpt-dev-ast` |
| Define a grammar and match it | `tpt-dev-dsl` |
| Generate Rust source from a build script | `tpt-dev-codegen` |

## Security and hardware

| I want to... | Use |
|---|---|
| Use ML-KEM or ML-DSA (post-quantum) | `tpt-dev-pqc` (not audited; see its README) |
| Write against a TEE interface, with a mock for tests | `tpt-dev-tee` (partial) |
| Simulate a hardware block cycle by cycle, with waveforms | `tpt-dev-sim` |

## AI and edge memory

| I want to... | Use |
|---|---|
| Manage LLM KV-cache pages in a fixed buffer | `tpt-dev-paged` |
| Schedule tasks across CPU and accelerator devices | `tpt-dev-dispatch` |

## Project tooling

| I want to... | Use |
|---|---|
| Run every project check in one command | `tpt-dev-cli` (`cargo tpt check`) |
| Lint for project rules without compiler plugins | `tpt-dev-lint` |
| Check dependency licences and cycles | `tpt-dev-dep` |
| Gate benchmarks in CI against a committed baseline | `tpt-dev-bench` |
| Run examples from doc comments in the browser | `tpt-dev-doc` |

## Not sure?

Start with the facade: `tpt-dev = { version = "0.0.0", features = ["verify"] }` (or another group) and use the module it exposes. Use single crates when you want a smaller dependency tree.

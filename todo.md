# tpt-dev: Project Todo

Maintainer: TPT Solutions · License: `MIT OR Apache-2.0` · 30 crates · Greenfield

**Ground rules (apply to every crate)**
- Our crates are dual-licensed `MIT OR Apache-2.0`. Dependencies must offer MIT (MIT, MIT OR Apache-2.0, BSD-2/3, ISC, Zlib, Unicode-3.0). No Apache-only, GPL or AGPL deps.
- `no_std` first; `std` behind an optional feature. Hot paths are zero-allocation.
- Runtime crates are zero-dep. Proc-macro crates may use syn/quote/proc-macro2. Dev-deps (loom, proptest, criterion) are allowed if they pass `cargo deny`.
- Every crate follows the standard checklist (S) below. Tasks listed under a crate are in addition to it.

**Standard per-crate checklist (S)**: design and public API sketch · implement core · `no_std` build on `thumbv7em-none-eabihf` · unit and integration tests · doc comment on every public item (with example or `include_str!("../README.md")`) · crate README · `cargo deny` clean · `0.1.0` publish.

Progress: Phase 0 [ ] · 1 [ ] · 2 [ ] · 3 [ ] · 4 [ ] · 5 [ ] · 6 [ ] · 7 [ ]

---

## Phase 0: Project setup & identity

- [ ] Check crates.io availability of all 30 `tpt-dev-*` names (and reserve them)
- [ ] Create GitHub org `tpt-solutions` and repo `tpt-dev`
- [ ] Initialise git repo and `.gitignore`
- [ ] Root `Cargo.toml`: workspace members `crates/*`, `[workspace.package]` (license `MIT OR Apache-2.0`, authors TPT Solutions, edition, repository, rust-version)
- [ ] `[workspace.dependencies]` with shared dependency definitions
- [ ] Shared `[workspace.lints]` (forbid/deny unsafe where possible, clippy settings)
- [ ] `rust-toolchain.toml` pinning stable Rust, with the `thumbv7em-none-eabihf` target
- [ ] Document the MSRV policy (decided: latest stable minus 2 releases; MSRV bumps only in minor versions and are noted in the changelog)
- [ ] `LICENSE-MIT` (copyright TPT Solutions)
- [ ] `LICENSE-APACHE` (Apache-2.0 text, copyright TPT Solutions)
- [ ] `deny.toml`: allow MIT, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0; leave Apache-2.0 off the allow list so only MIT-offering deps pass; deny Apache-2.0-only, GPL-1/2/3, AGPL-1/3
- [ ] Verify `deny.toml` accepts our own `MIT OR Apache-2.0` crates and an `MIT OR Apache-2.0` dep, and rejects an Apache-only dep
- [ ] `README.md`: high-level ecosystem overview, license section
- [ ] `CONTRIBUTING.md`: strict solo-maintainer boundaries, benevolent-dictator model
- [ ] Update `spec.txt` to match the dual-license decision (spec says strictly MIT)
- [ ] Write the missing spec section 5 (the spec jumps from 4 to 6)
- [ ] `crates/` skeleton: 30 empty crates with `Cargo.toml` (inherit workspace package fields), `src/lib.rs` with `#![no_std]`, `README.md`
- [ ] Add a `std` feature convention to every crate (`default = []`, `std = []`)
- [ ] Confirm the empty workspace builds on host and `thumbv7em-none-eabihf`

## Phase 1: CI/CD & quality gates

- [ ] GitHub Actions workflow: format & lint (`cargo fmt --check`, `cargo clippy --workspace -- -D warnings`)
- [ ] Workflow: license check (`cargo deny check licenses`), fails on any Apache-only or GPL dependency
- [ ] Workflow: `cargo test --workspace`
- [ ] Workflow: `cargo test --workspace --target thumbv7em-none-eabihf --no-run` (no_std compile check)
- [ ] Workflow: loom concurrency tests for all Group 3 crates (async, trace, lock, mem)
- [ ] Workflow: release pipeline using `cargo-release`
- [ ] Configure `cargo-release` (`cargo release patch --workspace --execute`: version bump, git tags, publish in dependency order)
- [ ] Configure `docs.rs` metadata per crate (`[package.metadata.docs.rs]`)
- [ ] Doc-comment rule check: every public function has a doc comment with a code example or `#[doc = include_str!("../README.md")]`
- [ ] Add `missing_docs` lint at workspace level
- [ ] Dry-run the full pipeline on the empty workspace

---

## Phase 2: Layer 0 primitives (Group 3 + AST)

### tpt-dev-mem: safe MMIO framework
- [ ] S
- [ ] Typed register/field abstractions with zero-cost access
- [ ] Read/write/modify access-mode types (RO, WO, RW, W1C)
- [ ] Volatile access wrapper with documented safety invariants
- [ ] Macro or builder to declare a peripheral register block
- [ ] Tests on host with a mock memory region
- [ ] Verify zero-cost via assembly or size comparison with raw volatile code

### tpt-dev-lock: deadlock-free sync primitives
- [ ] S
- [ ] Lock-ordering model for compile-time deadlock detection
- [ ] Lock-level types enforced by the type system
- [ ] Wait-free register / single-writer cells
- [ ] Lock-free SPSC queue (fixed capacity, no heap)
- [ ] Loom tests for every primitive
- [ ] Compile-fail tests (trybuild or equivalent) proving deadlock-prone code is rejected

### tpt-dev-trace: zero-alloc distributed tracing
- [ ] S
- [ ] Span and event types with fixed-size storage
- [ ] Trace and span ID propagation across tasks/devices
- [ ] Ring-buffer backed collector with no heap
- [ ] Pluggable `Sink` trait for exporting
- [ ] Loom tests for concurrent span recording
- [ ] Timestamp source abstraction for bare metal

### tpt-dev-sink: zero-alloc edge telemetry router
- [ ] S
- [ ] Lock-free, heap-free batching
- [ ] Routing rules table (static, fixed capacity)
- [ ] Back-pressure and drop policies
- [ ] Integration with tpt-dev-trace and tpt-dev-lock primitives
- [ ] Throughput benchmark on host

### tpt-dev-ast: zero-copy streaming AST parser framework
- [ ] S
- [ ] Streaming tokenizer API over `&[u8]` chunks
- [ ] Parser driver working in a fixed stack buffer (configurable depth)
- [ ] Error type for stack-depth overflow (no panics)
- [ ] Visitor/event API
- [ ] Example grammar (JSON-like) as a reference
- [ ] Test on inputs much larger than the stack buffer

---

## Phase 3: Layer 1 formal verification core (Group 1 + automata)

### tpt-dev-solver: pure-Rust QF_LRA SMT solver
- [ ] S
- [ ] Rational arithmetic type (decided: checked `i128` numerator/denominator, no external bignum; overflow returns `Unknown`, never a wrong answer)
- [ ] Term/formula representation (arena-allocated, fixed-capacity option)
- [ ] SMT-LIB 2 parser for the QF_LRA subset
- [ ] Simplex-based linear real arithmetic theory solver
- [ ] SAT core (CDCL) with DPLL(T) integration
- [ ] Model extraction
- [ ] Unsat core / proof output
- [ ] Incremental solving (push/pop)
- [ ] Cross-check against reference results on a test corpus (SMT-LIB QF_LRA benchmarks)
- [ ] Performance benchmarks and optimisation pass
- [ ] `no_std` + `alloc` support, with a bounded-memory mode

### tpt-dev-contract: `#[requires]` / `#[ensures]` macros
- [ ] S
- [ ] Proc-macro crate (syn/quote/proc-macro2)
- [ ] Parse `requires`, `ensures`, and `old()` expressions
- [ ] Translate the arithmetic subset to tpt-dev-solver queries
- [ ] Compile-time proof of contracts where decidable; clear compile errors on counterexamples
- [ ] Fall back to runtime checks (debug-only) for non-provable contracts
- [ ] Compile-fail tests with counterexample output
- [ ] Documentation of the supported expression subset

### tpt-dev-automata: state machine logic verifier
- [ ] S
- [ ] State machine description DSL (macro or builder)
- [ ] Reachability and deadlock analysis
- [ ] Compile-time proof of deadlock freedom via the solver where guards are arithmetic
- [ ] Counterexample trace reporting
- [ ] Compile-fail tests on deadlocking machines

### tpt-dev-test: property-based testing with invariant generation
- [ ] S
- [ ] Generator trait and shrinking
- [ ] Constraint-aware generation using tpt-dev-solver
- [ ] Invariant generation for arithmetic-dependent properties
- [ ] Deterministic seeds and failure reproduction
- [ ] `no_std` runner

### tpt-dev-fuzz: solver-guided structural fuzzer
- [ ] S
- [ ] Structure-aware input model
- [ ] Linear precondition solving to produce valid inputs
- [ ] Mutation strategy combined with solver-derived seeds
- [ ] Coverage feedback hook
- [ ] Corpus management and crash minimisation
- [ ] Demonstrate finding a bug behind a linear precondition that plain fuzzing misses

---

## Phase 4: Layer 2 runtime & codegen (Groups 3 + 4)

### tpt-dev-async: hard real-time no_std async runtime
- [ ] S
- [ ] Static task storage (no heap)
- [ ] Bounded-time executor with documented worst-case poll latency
- [ ] Priority scheduling
- [ ] Timers/delay abstraction on a hardware tick
- [ ] Wake mechanism safe from interrupt context
- [ ] Loom tests for executor and wakers
- [ ] WCET measurement on a target or simulator

### tpt-dev-replay: deterministic execution recorder
- [ ] S
- [ ] Event log format (compact, fixed-size ring buffer option)
- [ ] Record scheduling decisions, interrupts and inputs
- [ ] Deterministic replay engine
- [ ] Integration with tpt-dev-async and tpt-dev-trace
- [ ] Demonstrate replaying a concurrent crash bit-for-bit

### tpt-dev-codec: compile-time validated serialization
- [ ] S
- [ ] Schema definition (derive macro or DSL)
- [ ] Compile-time schema validation (overlaps, widths, versions)
- [ ] Zero-alloc encode/decode into caller buffers
- [ ] Endianness and bit-field support
- [ ] Fuzz round-trip tests

### tpt-dev-dsl: domain-specific language framework
- [ ] S
- [ ] Grammar definition API
- [ ] Parser generation built on tpt-dev-ast
- [ ] Macro embedding (`dsl!{...}`) with good error spans
- [ ] Example DSL as a reference

### tpt-dev-codegen: build-time code generation
- [ ] S
- [ ] Builder API for emitting Rust source
- [ ] Built-in formatting of generated code (no external rustfmt dependency)
- [ ] `build.rs` integration helpers and change detection
- [ ] Comparison examples against raw `build.rs` scripts

### tpt-dev-mock: compile-time zero-boilerplate mocking
- [ ] S
- [ ] Proc-macro `#[mock]` on traits
- [ ] Expectations, call counts and argument matchers
- [ ] `no_std` mock support
- [ ] Compile-time benchmark vs mockall

---

## Phase 5: Layer 3 FFI & developer tooling (Groups 2, 4, 5)

### tpt-dev-headers: C-header generator
- [ ] S
- [ ] Parse Rust `extern "C"` items and `#[repr(C)]` types
- [ ] Emit C headers with include guards and Doxygen comments from Rust docs
- [ ] Enum/struct/callback/opaque-handle support
- [ ] CLI and `build.rs` entry points
- [ ] Golden-file tests vs hand-written headers
- [ ] Compile generated headers with a C compiler in CI

### tpt-dev-ffi: zero-copy FFI state machine framework
- [ ] S
- [ ] Typestate API for stateful C lifetimes
- [ ] Ownership and borrow rules at the FFI boundary
- [ ] Handle/opaque-pointer helpers with validation
- [ ] Integration with tpt-dev-headers and tpt-dev-automata
- [ ] Example wrapping a stateful C library

### tpt-dev-lint: custom project-specific linting (no compiler plugins)
- [ ] S
- [ ] Rule definition API
- [ ] Source analysis without rustc plugins (decided: standalone tool built on syn)
- [ ] Rule configuration file
- [ ] Output compatible with Clippy-style diagnostics, and CI exit codes
- [ ] Ship a set of example rules
- [ ] Document the limits of analysis without type information

### tpt-dev-dep: dependency graph analyzer
- [ ] S
- [ ] Parse `Cargo.lock`/`cargo metadata` output
- [ ] Circular dependency detection
- [ ] License violation detection (consistent with deny.toml policy)
- [ ] Bloat analysis (dep count, transitive size)
- [ ] CLI and report formats

### tpt-dev-doc: interactive, runnable documentation
- [ ] S
- [ ] Extract examples from doc comments
- [ ] Embedded live REPL for running examples (decided: examples compiled to Wasm and run client-side in the browser, no server)
- [ ] Rustdoc integration (HTML injection)
- [ ] Hosting-compatible output for docs.rs
- [ ] Sandboxing/security review

### tpt-dev-profiler: zero-overhead sampling profiler
- [ ] S
- [ ] Sampling via timer/interrupt hook
- [ ] Function-level attribution without external `perf`
- [ ] Fixed-size sample buffer
- [ ] Report/flamegraph output
- [ ] Overhead measurement showing negligible cost

### tpt-dev-bench: statistical benchmarking & CI gating
- [ ] S
- [ ] Measurement harness with warm-up and outlier handling
- [ ] Statistical comparison (confidence intervals, significance tests)
- [ ] Baseline storage format
- [ ] CI gate: fail the PR on regression beyond threshold
- [ ] GitHub Actions example workflow
- [ ] `no_std` cycle-counter mode

---

## Phase 6: AI/edge, security & hardware (Groups 6, 7, 9, 10)

### tpt-dev-paged: no_std paged memory manager for LLM KV-caches
- [ ] S
- [ ] Fixed-size page pool allocator over a caller-provided buffer
- [ ] Per-sequence page tables
- [ ] Allocation, free, copy-on-write and eviction policies
- [ ] Fragmentation tests and metrics
- [ ] Benchmarks on host with a realistic KV-cache workload

### tpt-dev-dispatch: heterogeneous compute task scheduler
- [ ] S
- [ ] Device abstraction trait (CPU/NPU/GPU) with no vendor SDK dependency
- [ ] Task graph and cost model
- [ ] Scheduler routing tasks to devices
- [ ] CPU reference backend
- [ ] Mock accelerator backends for testing

### tpt-dev-pqc: pure-Rust post-quantum cryptography
- [ ] S
- [ ] ML-KEM (FIPS 203): 512/768/1024
- [ ] ML-DSA (FIPS 204): 44/65/87
- [ ] Constant-time implementation, with verification (e.g. dudect-style tests)
- [ ] NIST KAT vectors pass
- [ ] Zeroisation of secrets
- [ ] Bring-your-own-RNG interface (no_std)
- [ ] Security review and a clear "not independently audited" notice
- [ ] Performance benchmarks on host and Cortex-M

### tpt-dev-zk: Rust-to-zero-knowledge circuit transpiler
- [ ] S
- [ ] Circuit IR (decided: R1CS, with export to the circom `.r1cs` format; proof-system backends stay external)
- [ ] Rust function front end (proc-macro) for the supported subset
- [ ] Transpile to circuit IR
- [ ] Witness generation
- [ ] Circuit export format
- [ ] Tests comparing circuit output with native Rust execution
- [ ] Document the supported language subset

### tpt-dev-tee: Trusted Execution Environment abstraction
- [ ] S
- [ ] Common TEE trait set (secure memory, attestation, secure calls)
- [ ] ARM TrustZone backend (Cortex-M NS/S transitions)
- [ ] Intel TDX backend
- [ ] Mock TEE for host tests
- [ ] Safety review of secure/non-secure boundary code

### tpt-dev-compose: Wasm Component Model linker
- [ ] S
- [ ] Wasm binary/component parser (pure Rust)
- [ ] Component Model type system and canonical ABI
- [ ] Instance linking and composition at runtime
- [ ] Import/export resolution and validation
- [ ] Conformance tests against sample components

### tpt-dev-sim: cycle-accurate hardware simulator
- [ ] S
- [ ] Signal/clock/event kernel (fixed-capacity option)
- [ ] Cycle-accurate scheduling model
- [ ] Component/module description API in safe Rust
- [ ] Waveform output (e.g. VCD)
- [ ] Example: a small CPU or peripheral model with timing checks
- [ ] Integration with tpt-dev-mem for MMIO modelling

---

## Phase 7: Release & maintenance

- [ ] Cross-crate dependency and version audit
- [ ] Final `cargo deny check` across the whole workspace
- [ ] Confirm every crate builds on `thumbv7em-none-eabihf` and passes loom where applicable
- [ ] Confirm every public function documented with example or README include
- [ ] Publish 0.x in dependency order (decided: `cargo-release` resolves it; layers 0, 1, 2, 3, then Phase 6 crates)
- [ ] First release via `cargo release` for all 30 crates
- [ ] Verify the docs.rs builds for all 30 crates
- [ ] Final workspace `README.md` ecosystem overview with a crate index
- [ ] Semantic versioning policy documented
- [ ] 1.0 API-stability review per crate (1.0 signals stability)
- [ ] Ongoing: patch release process via `cargo release patch --workspace --execute`

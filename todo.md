# tpt-dev: Project Todo

Maintainer: TPT Solutions · License: `MIT OR Apache-2.0` · 30 crates · Greenfield

**Ground rules (apply to every crate)**
- Our crates are dual-licensed `MIT OR Apache-2.0`. Dependencies must offer MIT (MIT, MIT OR Apache-2.0, BSD-2/3, ISC, Zlib, Unicode-3.0). No Apache-only, GPL or AGPL deps.
- `no_std` first; `std` behind an optional feature. Hot paths are zero-allocation.
- Runtime crates are zero-dep. Proc-macro crates may use syn/quote/proc-macro2. Dev-deps (loom, proptest, criterion) are allowed if they pass `cargo deny`.
- Every crate follows the standard checklist (S) below. Tasks listed under a crate are in addition to it.

**Standard per-crate checklist (S)**: design and public API sketch · implement core · `no_std` build on `thumbv7em-none-eabihf` · unit and integration tests · doc comment on every public item (with example or `include_str!("../README.md")`) · crate README · `cargo deny` clean · `0.1.0` publish.

Progress: Phase 0 [ ] · 1 [ ] · 2 [x] · 3 [ ] · 4 [ ] · 5 [ ] · 6 [ ] · 7 [ ]

---

## Phase 0: Project setup & identity

- [ ] Check crates.io availability of all 30 `tpt-dev-*` names (and reserve them) — *availability checked 2026-11-19: all 30 unclaimed; reservation pending (needs a publish, so left for the maintainer)*
- [x] Create GitHub org `tpt-solutions` and repo `tpt-dev` — *repo exists at `origin` (verified with `git ls-remote`)*
- [x] Initialise git repo and `.gitignore`
- [x] Root `Cargo.toml`: workspace members `crates/*`, `[workspace.package]` (license `MIT OR Apache-2.0`, authors TPT Solutions, edition, repository, rust-version)
- [ ] `[workspace.dependencies]` with shared dependency definitions — *deferred: cargo warns about unused entries, so add them with the first proc-macro crate (syn/quote/proc-macro2)*
- [x] Shared `[workspace.lints]` (forbid/deny unsafe where possible, clippy settings) — *`unsafe_code = deny`, not `forbid`, so a crate can opt in later with a SAFETY comment*
- [x] `rust-toolchain.toml` pinning stable Rust, with the `thumbv7em-none-eabihf` target
- [x] Document the MSRV policy (decided: latest stable minus 2 releases; MSRV bumps only in minor versions and are noted in the changelog) — *in CONTRIBUTING.md; `rust-version = 1.98`*
- [x] `LICENSE-MIT` (copyright TPT Solutions)
- [x] `LICENSE-APACHE` (Apache-2.0 text, copyright TPT Solutions)
- [x] `deny.toml`: allow MIT, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0; leave Apache-2.0 off the allow list so only MIT-offering deps pass; deny Apache-2.0-only, GPL-1/2/3, AGPL-1/3
- [x] Verify `deny.toml` accepts our own `MIT OR Apache-2.0` crates and an `MIT OR Apache-2.0` dep, and rejects an Apache-only dep — *verified with scratch crates (Apache-only and GPL rejected; dual and MIT accepted)*
- [x] `README.md`: high-level ecosystem overview, license section
- [x] `CONTRIBUTING.md`: strict solo-maintainer boundaries, benevolent-dictator model
- [x] Update `spec.txt` to match the dual-license decision (spec says strictly MIT)
- [x] Write the missing spec section 5 (the spec jumps from 4 to 6) — *written as "Implementation Layers & Build Order", derived from Phases 2–6*
- [x] `crates/` skeleton: 30 empty crates with `Cargo.toml` (inherit workspace package fields), `src/lib.rs` with `#![no_std]`, `README.md`
- [x] Add a `std` feature convention to every crate (`default = []`, `std = []`)
- [x] Confirm the empty workspace builds on host and `thumbv7em-none-eabihf`

## Phase 1: CI/CD & quality gates

- [x] GitHub Actions workflow: format & lint (`cargo fmt --check`, `cargo clippy --workspace -- -D warnings`) — *`.github/workflows/ci.yml`*
- [x] Workflow: license check (`cargo deny check licenses`), fails on any Apache-only or GPL dependency
- [x] Workflow: `cargo test --workspace`
- [x] Workflow: no_std compile check — *uses `cargo build --workspace --target thumbv7em-none-eabihf`; `cargo test --target thumbv7em…` cannot work because test harnesses need std*
- [ ] Workflow: loom concurrency tests for all Group 3 crates (async, trace, lock, mem) — *deferred until those crates have code*
- [x] Workflow: release pipeline using `cargo-release` — *`.github/workflows/release.yml`, manual dispatch, dry run by default*
- [x] Configure `cargo-release` (`cargo release patch --workspace --execute`: version bump, git tags, publish in dependency order) — *`release.toml`, shared version*
- [x] Configure `docs.rs` metadata per crate (`[package.metadata.docs.rs]`)
- [ ] Doc-comment rule check: every public function has a doc comment with a code example or `#[doc = include_str!("../README.md")]` — *deferred: needs public items to check; a candidate for tpt-dev-lint*
- [x] Add `missing_docs` lint at workspace level
- [ ] Dry-run the full pipeline on the empty workspace — *local checks pass (fmt, clippy -D warnings, test, thumbv7em build, cargo-deny). `cargo release patch --workspace --no-verify` plans v0.0.1 and reaches the push step, which fails in dry run because the tree is uncommitted. Re-run on a committed tree. GitHub Actions not run remotely.*

---

## Phase 2: Layer 0 primitives (Group 3 + AST)

### tpt-dev-mem: safe MMIO framework
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Typed register/field abstractions with zero-cost access
- [x] Read/write/modify access-mode types (RO, WO, RW, W1C)
- [x] Volatile access wrapper with documented safety invariants
- [x] Macro or builder to declare a peripheral register block
- [x] Tests on host with a mock memory region
- [x] Verify zero-cost via assembly or size comparison with raw volatile code — *typed `modify_field` and raw read-modify-write emit identical Thumb assembly (thumbv7em, release)*

### tpt-dev-lock: deadlock-free sync primitives
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Lock-ordering model for compile-time deadlock detection
- [x] Lock-level types enforced by the type system
- [x] Wait-free register / single-writer cells
- [x] Lock-free SPSC queue (fixed capacity, no heap)
- [x] Loom tests for every primitive — *`tests/loom.rs`: TripleBuffer, Spsc, nested Mutex; run with `RUSTFLAGS="--cfg loom"`*
- [x] Compile-fail tests (trybuild or equivalent) proving deadlock-prone code is rejected — *trybuild in `tests/ui/`*

### tpt-dev-trace: zero-alloc distributed tracing
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Span and event types with fixed-size storage
- [x] Trace and span ID propagation across tasks/devices
- [x] Ring-buffer backed collector with no heap
- [x] Pluggable `Sink` trait for exporting
- [x] Loom tests for concurrent span recording — *`tests/loom.rs`; takes ~80 s*
- [x] Timestamp source abstraction for bare metal

### tpt-dev-sink: zero-alloc edge telemetry router
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Lock-free, heap-free batching
- [x] Routing rules table (static, fixed capacity)
- [x] Back-pressure and drop policies
- [x] Integration with tpt-dev-trace and tpt-dev-lock primitives
- [x] Throughput benchmark on host — *`examples/throughput.rs`: ~470 M msgs/s direct, ~420 M msgs/s via SPSC ingress (release, host)*

### tpt-dev-ast: zero-copy streaming AST parser framework
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Streaming tokenizer API over `&[u8]` chunks
- [x] Parser driver working in a fixed stack buffer (configurable depth)
- [x] Error type for stack-depth overflow (no panics)
- [x] Visitor/event API
- [x] Example grammar (JSON-like) as a reference
- [x] Test on inputs much larger than the stack buffer

---

## Phase 3: Layer 1 formal verification core (Group 1 + automata)

### tpt-dev-solver: pure-Rust QF_LRA SMT solver
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Rational arithmetic type (decided: checked `i128` numerator/denominator, no external bignum; overflow returns `Unknown`, never a wrong answer) — *`rational.rs`; comparison is overflow-free (continued fractions)*
- [ ] Term/formula representation (arena-allocated, fixed-capacity option) — *arena is `alloc`-backed (`term.rs`); the fixed-capacity option is not done*
- [x] SMT-LIB 2 parser for the QF_LRA subset — *`smt.rs`: declare, assert, check-sat, get-model, push/pop, exit; nonlinear terms rejected*
- [x] Simplex-based linear real arithmetic theory solver — *Dutertre–de Moura with delta-rationals, Bland's rule (`simplex.rs`)*
- [x] SAT core (CDCL) with DPLL(T) integration — *lazy: theory checked on full assignments; blocking clauses from simplex conflicts (`sat.rs`)*
- [x] Model extraction
- [ ] Unsat core / proof output — *minimal unsat core done (`unsat_core`, deletion-based); proof output not done*
- [ ] Incremental solving (push/pop) — *push/pop scope the assertions, but every check re-solves from scratch; not yet truly incremental*
- [ ] Cross-check against reference results on a test corpus (SMT-LIB QF_LRA benchmarks) — *randomized oracle (DNF + Fourier–Motzkin) passes about 50k random cases (several seeds and depths) with model checks; SMT-LIB corpus not yet available offline*
- [ ] Performance benchmarks and optimisation pass
- [ ] `no_std` + `alloc` support, with a bounded-memory mode — *`no_std` + `alloc` builds for `thumbv7em`; bounded-memory mode not done*

### tpt-dev-contract: `#[requires]` / `#[ensures]` macros
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Proc-macro crate (syn/quote/proc-macro2) — *`proc-macro = true`; host only, so CI excludes it from the bare-metal build*
- [ ] Parse `requires`, `ensures`, and `old()` expressions — *`requires` and `ensures` parse; `old()` is not supported yet*
- [x] Translate the arithmetic subset to tpt-dev-solver queries — *`translate.rs`: linear integer arithmetic and booleans*
- [x] Compile-time proof of contracts where decidable; clear compile errors on counterexamples — *proves `ensures` for straight-line linear bodies; refutations with integer models become compile errors naming the inputs*
- [x] Fall back to runtime checks (debug-only) for non-provable contracts — *`requires` is always checked on entry; unproven `ensures` are checked on return*
- [x] Compile-fail tests with counterexample output — *trybuild case in `tests/ui/refuted.rs`*
- [x] Documentation of the supported expression subset — *`README.md`, with limits*

### tpt-dev-automata: state machine logic verifier
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] State machine description DSL (macro or builder) — *builder API in `machine.rs`; a `macro_rules!` front end is not written*
- [x] Reachability and deadlock analysis — *BFS reachability ignores guards (over-approximation, documented); deadlocks from missing transitions or guards that can all be false*
- [ ] Compile-time proof of deadlock freedom via the solver where guards are arithmetic — *analysis runs at run time; a compile-time check needs a proc macro (not written)*
- [x] Counterexample trace reporting — *shortest event path to each deadlock, plus a solver witness valuation*
- [ ] Compile-fail tests on deadlocking machines — *pending the compile-time check*

### tpt-dev-test: property-based testing with invariant generation
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Generator trait and shrinking — *`generator.rs`: IntRange, Bools, VecOf, Pair; greedy shrinking in `runner.rs`*
- [x] Constraint-aware generation using tpt-dev-solver — *`ConstrainedGen` samples models of a linear constraint with random bounds*
- [ ] Invariant generation for arithmetic-dependent properties — *`infer` proposes sample-derived bounds on single variables and pairs; `prove` checks each with the solver (counterexamples are confirmed). No inductive-invariant search over transition systems yet.*
- [x] Deterministic seeds and failure reproduction — *per-case seeds; `Failure` reports seed and case; `replay` regenerates the input*
- [x] `no_std` runner — *`no_std` + `alloc`; no std dependency*

### tpt-dev-fuzz: solver-guided structural fuzzer
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Structure-aware input model — *fixed-arity integer inputs with bounded mutation (`fuzzer.rs`); no byte-level grammar*
- [x] Linear precondition solving to produce valid inputs — *`solver_seeds` samples integral solver models of a linear precondition (`seeds.rs`); fractional models are rejected*
- [x] Mutation strategy combined with solver-derived seeds — *random step, uniform, corpus-copy and swap mutations; solver seeds start the corpus*
- [x] Coverage feedback hook — *targets call `Coverage::hit`; inputs with new edges join the corpus (`coverage.rs`)*
- [x] Corpus management and crash minimisation — *bounded corpus with random replacement; greedy minimisation toward zero that keeps the crash*
- [x] Demonstrate finding a bug behind a linear precondition that plain fuzzing misses — *`tests/guided.rs`: plain fuzzing misses a sum-equality bug in 20,000 iterations; solver seeds find it on the first run*

---

## Phase 4: Layer 2 runtime & codegen (Groups 3 + 4)

### tpt-dev-async: hard real-time no_std async runtime
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Static task storage (no heap) — *tasks borrowed from caller storage; fixed slots, up to 32*
- [x] Bounded-time executor with documented worst-case poll latency — *a round polls at most N tasks; latency bounds in README*
- [x] Priority scheduling — *highest-priority ready task first within a round*
- [x] Timers/delay abstraction on a hardware tick — *`Delay` over `tpt_dev_trace::Clock`; it busy-waits (self-wakes), so no sleep-until-tick yet*
- [x] Wake mechanism safe from interrupt context — *wake is one atomic fetch_or, no lock or allocation*
- [x] Loom tests for executor and wakers — *`tests/loom.rs`: no lost wake-ups from another thread*
- [ ] WCET measurement on a target or simulator — *`Stats` counts polls on the host; target measurement not done*

### tpt-dev-replay: deterministic execution recorder
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Event log format (compact, fixed-size ring buffer option) — *16-byte events; `Log` (append-only, replayable) and `Ring` (overwriting, counts losses)*
- [x] Record scheduling decisions, interrupts and inputs — *`Session` and `step` log each interrupt, each poll and each input*
- [x] Deterministic replay engine — *a replay follows the log and reports `Divergence` on any mismatch*
- [x] Integration with tpt-dev-async and tpt-dev-trace — *drives `poll_slot`/`wake_slot` on the async executor; `Event::to_trace_record` exports to trace*
- [x] Demonstrate replaying a concurrent crash bit-for-bit — *`tests/crash.rs`: a recorded crash replays with the same message and an identical log*

### tpt-dev-codec: compile-time validated serialization
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Schema definition (derive macro or DSL) — *`const` field lists (`Field::bits`/`Field::be`); no derive macro*
- [x] Compile-time schema validation (overlaps, widths, versions) — *checked in const evaluation; `Layout::extends` checks version evolution; trybuild cases*
- [x] Zero-alloc encode/decode into caller buffers — *`encode`/`decode` on slices, no allocation*
- [x] Endianness and bit-field support — *little-endian bit fields; byte-aligned big-endian fields*
- [x] Fuzz round-trip tests — *property-based round trips via tpt-dev-test (not coverage-guided)*

### tpt-dev-dsl: domain-specific language framework
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Grammar definition API — *`Grammar`, `Rule`, `Expr` (PEG), checked at compile time in a `const`*
- [ ] Parser generation built on tpt-dev-ast — *not done: the ast crate's lexer is JSON-only; the DSL interprets its own grammars*
- [ ] Macro embedding (`dsl!{...}`) with good error spans — *not done: needs a proc-macro crate*
- [x] Example DSL as a reference — *arithmetic grammar in `tests/arith.rs`, described in the README*

### tpt-dev-codegen: build-time code generation
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Builder API for emitting Rust source — *`Writer`: lines, blocks, constants, escaped string constants*
- [x] Built-in formatting of generated code (no external rustfmt dependency) — *layout is structural (indentation from block nesting); arbitrary code is not reformatted*
- [x] `build.rs` integration helpers and change detection — *`build::write_if_changed`, `rerun_if_changed`, `rerun_if_env_changed`*
- [x] Comparison examples against raw `build.rs` scripts — *`examples/compare.rs` checks that raw and builder output match*

### tpt-dev-mock: compile-time zero-boilerplate mocking
*Layout note: the proc macro lives in `tpt-dev-mock-macros`, a companion proc-macro crate. This is the 31st crate in the workspace, and it is accepted (see spec section 3).*
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Proc-macro `#[mock]` on traits — *in `tpt-dev-mock-macros` (a proc-macro crate), re-exported by `tpt-dev-mock`*
- [x] Expectations, call counts and argument matchers — *`with` matchers, `times`, `returns`/`returns_with`, `verify`, `calls`*
- [x] `no_std` mock support — *runtime is `no_std` with `alloc`; builds for `thumbv7em`*
- [ ] Compile-time benchmark vs mockall — *not run: mockall is not a dependency, so no comparison yet*

---

## Phase 5: Layer 3 FFI & developer tooling (Groups 2, 4, 5)

### tpt-dev-headers: C-header generator
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Parse Rust `extern "C"` items and `#[repr(C)]` types — *`syn`-based; functions, repr(C) structs, integer enums, aliases, constants*
- [x] Emit C headers with include guards and Doxygen comments from Rust docs — *include guard set by the caller; `/** */` blocks on items and fields*
- [x] Enum/struct/callback/opaque-handle support — *opaque structs, function-pointer typedefs (`extern "C" fn`)*
- [ ] CLI and `build.rs` entry points — *`generate(source, options)` is the entry point for build scripts; no CLI binary yet*
- [x] Golden-file tests vs hand-written headers — *golden test against a generated-then-reviewed `api.h` (not hand-written); `BLESS=1` updates it*
- [ ] Compile generated headers with a C compiler in CI — *not done: no C compiler is available here, and the CI job is not written*

### tpt-dev-ffi: zero-copy FFI state machine framework
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Typestate API for stateful C lifetimes — *`Closed`/`Open`/`Streaming` sessions; transitions consume the session (`session.rs`); compile-fail case
- [ ] Ownership and borrow rules at the FFI boundary — *partial: the session owns the object and frees it on drop; buffers are borrowed for one call (`with`); no zero-copy views that outlive a call*
- [x] Handle/opaque-pointer helpers with validation — *`Handle<T>` is non-null and typed*
- [ ] Integration with tpt-dev-headers and tpt-dev-automata — *not done*
- [x] Example wrapping a stateful C library — *`tests/c_library.rs`: a Rust library exported through `extern "C"`; no C compiler is available, so no real C was compiled*

### tpt-dev-lint: custom project-specific linting (no compiler plugins)
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Rule definition API — *`Rule` trait over `syn` syntax trees, with findings and severities (`lib.rs`)*
- [x] Source analysis without rustc plugins (decided: standalone tool built on syn) — *syntactic analysis via `syn::visit`; no type information*
- [x] Rule configuration file — *`rule = allow|warn|deny` lines (`config.rs`); unknown rule names are errors*
- [x] Output compatible with Clippy-style diagnostics, and CI exit codes — *`severity[rule]: message` with location; exit 1 on deny, 2 on usage errors*
- [x] Ship a set of example rules — *`no-unwrap`, `no-todo-macros`, `safety-comment`*
- [x] Document the limits of analysis without type information — *README limits section*

### tpt-dev-dep: dependency graph analyzer
- [x] S — *0.1.0 publish deferred to Phase 7*
- [ ] Parse `Cargo.lock`/`cargo metadata` output — *`Cargo.lock` only; `cargo metadata` JSON is not parsed*
- [x] Circular dependency detection — *Tarjan's strongly connected components (`report.rs`)*
- [x] License violation detection (consistent with deny.toml policy) — *SPDX-style expressions checked against an allow list; the list is passed in, not read from deny.toml*
- [x] Bloat analysis (dep count, transitive size) — *transitive dependency counts per package; sizes in bytes are not measured*
- [x] CLI and report formats — *`tpt-dev-dep` binary with text output and exit codes; no JSON output*

### tpt-dev-doc: interactive, runnable documentation
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Extract examples from doc comments — *`extract` reads fenced blocks with rustdoc's tags and hidden lines; `doc_comments` takes `///` and `//!` text from source*
- [ ] Embedded live REPL for running examples (decided: examples compiled to Wasm and run client-side in the browser, no server) — *not done: examples are shown, not run. Needs the `wasm32-unknown-unknown` target (not installed here) and a browser runtime to test.*
- [ ] Rustdoc integration (HTML injection) — *`inject_before` inserts a fragment before a marker, and reports a missing marker; no build step wires it into `cargo doc` yet*
- [ ] Hosting-compatible output for docs.rs — *`page` writes a self-contained HTML page with no script or network reference; no build step writes it into a crate's docs yet*
- [x] Sandboxing/security review — *README section: nothing executes code yet; the review must be repeated when the Wasm runner lands*

### tpt-dev-profiler: sampling profiler (bounded per-sample cost)
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Sampling via timer/interrupt hook — *`Sampler::record` is lock-free and does not allocate; the timer and interrupt wiring is the board's, not the crate's*
- [x] Function-level attribution without external `perf` — *`Symbols`: a caller-supplied sorted table with the code end; `Report` gives self and inclusive counts*
- [x] Fixed-size sample buffer — *`Sampler<N, D>`: atomics, append-only, drop-and-count when full; a reader skips unpublished slots*
- [x] Report/flamegraph output — *text table and folded stacks for `inferno`/`flamegraph.pl`; no SVG is rendered in the crate*
- [ ] Overhead measurement showing negligible cost — *`examples/overhead.rs` measures ~9 ns per sample on the host (release, 5-frame stack; 0.009% of a core at 10 kHz). Not measured on a target, so "negligible" is not yet shown for hardware.*

### tpt-dev-bench: statistical benchmarking & CI gating
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Measurement harness with warm-up and outlier handling — *`harness::measure` (std) with warm-up and `black_box`; `trim_outliers` (Tukey)*
- [x] Statistical comparison (confidence intervals, significance tests) — *Welch's t-test; the critical value is approximate and no confidence interval is computed*
- [x] Baseline storage format — *plain text `name mean stddev n`, parsed and written by `Baseline`*
- [x] CI gate: fail the PR on regression beyond threshold — *`judge` with `Policy`; a trusted regression is a failure*
- [ ] GitHub Actions example workflow — *not written*
- [ ] `no_std` cycle-counter mode — *not written; the core is no_std but the harness uses Instant*

---

## Phase 6: AI/edge, security & hardware (Groups 6, 7, 9, 10)

### tpt-dev-paged: no_std paged memory manager for LLM KV-caches
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Fixed-size page pool allocator over a caller-provided buffer — *`Pager::new(storage, tokens_per_page, bytes_per_token)`; no heap*
- [x] Per-sequence page tables — *fixed-size tables of up to MAXP pages per sequence*
- [x] Allocation, free, copy-on-write and eviction policies — *`append`, `close`, `fork` with COW on write, LRU eviction of whole sequences*
- [x] Fragmentation tests and metrics — *no external fragmentation with fixed pages; `internal_fragmentation` and `stats`*
- [x] Benchmarks on host with a realistic KV-cache workload — *`examples/kv_workload.rs` (decode, fork, close, eviction); host bookkeeping only*

### tpt-dev-dispatch: heterogeneous compute task scheduler
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Device abstraction trait (CPU/NPU/GPU) with no vendor SDK dependency — *`Device` trait with `Kind::{Cpu,Npu,Gpu}`; no GPU backend exists yet*
- [x] Task graph and cost model — *`Graph` of vector ops with validation; per-device estimated cost from input size*
- [x] Scheduler routing tasks to devices — *earliest-finish-time list scheduling; `schedule` and `execute`*
- [x] CPU reference backend — *`CpuDevice` runs every operation*
- [x] Mock accelerator backends for testing — *one `MockAccelerator` (NPU kind) that records calls; a second mock is not written*

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
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Signal/clock/event kernel (fixed-capacity option) — *`Kernel<N>` holds up to N signals in arrays and allocates nothing; single clock; no event queue (cycle-based only)*
- [x] Cycle-accurate scheduling model — *two-phase cycle: components read committed values and write next values; writes commit together, so order does not matter; every value is registered (one-cycle delay)*
- [x] Component/module description API in safe Rust — *`Component::tick` with `Ctx` (`read`, `write`, `fail`); `step` takes a list of components*
- [x] Waveform output (e.g. VCD) — *`Vcd` writes to any `core::fmt::Write`*
- [x] Example: a small CPU or peripheral model with timing checks — *`timer::Timer` peripheral with `Deadline` and `MinGap` checks; no CPU model*
- [x] Integration with tpt-dev-mem for MMIO modelling — *`tests/timer_mmio.rs`: firmware uses `tpt-dev-mem` `Reg` handles on the timer's register memory*

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

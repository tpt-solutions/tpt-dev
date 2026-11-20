# tpt-dev: Project Todo

Maintainer: TPT Solutions · License: `MIT OR Apache-2.0` · 30 crates · Greenfield

**Ground rules (apply to every crate)**
- Our crates are dual-licensed `MIT OR Apache-2.0`. Dependencies must offer MIT (MIT, MIT OR Apache-2.0, BSD-2/3, ISC, Zlib, Unicode-3.0). No Apache-only, GPL or AGPL deps.
- `no_std` first; `std` behind an optional feature. Hot paths are zero-allocation.
- Runtime crates are zero-dep. Proc-macro crates may use syn/quote/proc-macro2. Dev-deps (loom, proptest, criterion) are allowed if they pass `cargo deny`.
- Every crate follows the standard checklist (S) below. Tasks listed under a crate are in addition to it.

**Standard per-crate checklist (S)**: design and public API sketch · implement core · `no_std` build on `thumbv7em-none-eabihf` · unit and integration tests · doc comment on every public item (with example or `include_str!("../README.md")`) · crate README · `cargo deny` clean · `0.1.0` publish.

Progress: Phase 0 [ ] · 1 [ ] · 2 [x] · 3 [ ] · 4 [ ] · 5 [ ] · 6 [ ] · 7 [ ] · 8 [ ]

---

## Phase 0: Project setup & identity

- [ ] Check crates.io availability of all 30 `tpt-dev-*` names (and reserve them) — *availability checked 2026-11-19: all 30 unclaimed; reservation pending (needs a publish, so left for the maintainer)*
- [x] Create GitHub org `tpt-solutions` and repo `tpt-dev` — *repo exists at `origin` (verified with `git ls-remote`)*
- [x] Initialise git repo and `.gitignore`
- [x] Root `Cargo.toml`: workspace members `crates/*`, `[workspace.package]` (license `MIT OR Apache-2.0`, authors TPT Solutions, edition, repository, rust-version)
- [x] `[workspace.dependencies]` with shared dependency definitions — *`loom`, `proc-macro2`, `quote`, `syn`; member crates use `workspace = true` and keep their own feature lists*
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
- [x] Workflow: loom concurrency tests for Group 3 crates (async, trace, lock) — *`loom` job in `ci.yml`; mem has no concurrent code, so no loom tests. Passes locally (trace takes about 200 s)*
- [x] Workflow: release pipeline using `cargo-release` — *`.github/workflows/release.yml`, manual dispatch, dry run by default*
- [x] Configure `cargo-release` (`cargo release patch --workspace --execute`: version bump, git tags, publish in dependency order) — *`release.toml`, shared version*
- [x] Configure `docs.rs` metadata per crate (`[package.metadata.docs.rs]`)
- [x] Doc-comment rule check: every public function has a doc comment with a code example — *`tpt-dev-lint` `doc-example` rule (warn by default); pub free fns and inherent pub methods only; 460 findings across the workspace, not fixed*
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
- [x] Term/formula representation (arena-allocated, fixed-capacity option) — *heap arena plus an allocation-free `FixedArena`; the search itself still uses `alloc`*
- [x] SMT-LIB 2 parser for the QF_LRA subset — *`smt.rs`: declare, assert, check-sat, get-model, push/pop, exit; nonlinear terms rejected*
- [x] Simplex-based linear real arithmetic theory solver — *Dutertre–de Moura with delta-rationals, Bland's rule (`simplex.rs`)*
- [x] SAT core (CDCL) with DPLL(T) integration — *lazy: theory checked on full assignments; blocking clauses from simplex conflicts (`sat.rs`)*
- [x] Model extraction
- [x] Unsat core / proof output — *unsat core (`unsat_core`, deletion-based); Farkas certificates for conjunctive unsat (`unsat_certificate`, `check_farkas`); DRUP-style proofs for Boolean-structured unsat (`unsat_proof`, checked by `check_proof`). The Tseitin encoder is checked by exhaustive equivalence tests up to 16 CNF variables, not proved for larger sizes.*
- [x] Incremental solving (push/pop) — *clause database, learned clauses and simplex tableau persist across checks; checked against from-scratch solves on random push/pop sequences. No clause deletion.*
- [ ] Cross-check against reference results on a test corpus (SMT-LIB QF_LRA benchmarks) — *randomized oracle (DNF + Fourier–Motzkin) passes about 50k random cases (several seeds and depths) with model checks; SMT-LIB official corpus not reachable. Cross-check on 14 of 46 QF_LRA files from the cvc5/Z3 archives (`tests/smtlib_external.rs`, opt-in via `TPT_SMTLIB_DIR`): 14 agree, 0 disagree. Unsupported: 20 with Bool variables, 5 with real-valued `ite`, 5 with `distinct`/`xor`, 1 over the size cap.*
- [x] Performance benchmarks and optimisation pass — *`examples/bench.rs`; tableau reuse cut disjunctive v=20 from 579 ms to 119 ms. Branch selection and rational gcd remain hotspots.*
- [x] `no_std` + `alloc` support, with a bounded-memory mode — *`no_std` + `alloc` builds for `thumbv7em`; `Budget` (formula nodes, clauses, pivots) returns Unknown. Encoder recursion depth is not capped.*

### tpt-dev-contract: `#[requires]` / `#[ensures]` macros
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Proc-macro crate (syn/quote/proc-macro2) — *`proc-macro = true`; host only, so CI excludes it from the bare-metal build*
- [x] Parse `requires`, `ensures`, and `old()` expressions — *`old(x)` is the entry snapshot in both the solver and run-time checks; it must be a Copy integer parameter; `old` in `requires` is rejected*
- [x] Translate the arithmetic subset to tpt-dev-solver queries — *`translate.rs`: linear integer arithmetic and booleans*
- [x] Compile-time proof of contracts where decidable; clear compile errors on counterexamples — *proves `ensures` for straight-line linear bodies; refutations with integer models become compile errors naming the inputs*
- [x] Fall back to runtime checks (debug-only) for non-provable contracts — *`requires` is always checked on entry; unproven `ensures` are checked on return*
- [x] Compile-fail tests with counterexample output — *trybuild case in `tests/ui/refuted.rs`*
- [x] Documentation of the supported expression subset — *`README.md`, with limits*

### tpt-dev-automata: state machine logic verifier
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] State machine description DSL (macro or builder) — *builder API in `machine.rs`; a `macro_rules!` front end is not written*
- [x] Reachability and deadlock analysis — *BFS reachability ignores guards (over-approximation, documented); deadlocks from missing transitions or guards that can all be false*
- [x] Compile-time proof of deadlock freedom for box guards — *`const_check::assert_deadlock_free` runs in a const context. Integer thresholds only, 64 states and 4 variables. General arithmetic guards are still checked only at run time.*
- [x] Counterexample trace reporting — *shortest event path to each deadlock, plus a solver witness valuation*
- [x] Compile-fail tests on deadlocking machines — *trybuild in `tests/ui/`: missing transition and guard gap*

### tpt-dev-test: property-based testing with invariant generation
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Generator trait and shrinking — *`generator.rs`: IntRange, Bools, VecOf, Pair; greedy shrinking in `runner.rs`*
- [x] Constraint-aware generation using tpt-dev-solver — *`ConstrainedGen` samples models of a linear constraint with random bounds*
- [x] Invariant generation for arithmetic-dependent properties — *`find_invariant`: sampled bounds, Houdini filtering, solver-checked initiation/consecution/property. Violations come with a concrete trace. Limits: no disjunctive or non-linear invariants, no counterexample strengthening, integer systems checked over the reals.*
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
- [x] Parser generation built on tpt-dev-ast — *`PackratStream` feeds byte chunks through tpt-dev-ast `Feed` and `GrammarLexer`, and matches after `finish`. Equal to `parse_packrat` on random chunked arithmetic input. Matching is now incremental (`resumable.rs`, ring-buffered); the returned tree is still built in full.*
- [x] Macro embedding (`dsl!{...}`) with good error spans — *`tpt-dev-dsl-macros`, re-exported as `tpt_dev_dsl::dsl` behind feature `macros`; trybuild compile-fail tests in `tests/ui`.*
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
- [x] Compile-time benchmark vs mockall — *mockall 0.15 dev-dep; `examples/compile_bench.rs` at 10 and 50 methods. A 50-method rebuild takes 1.94x tpt-dev-mock's time (one machine; the 10-method gap is within noise).*

---

## Phase 5: Layer 3 FFI & developer tooling (Groups 2, 4, 5)

### tpt-dev-headers: C-header generator
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Parse Rust `extern "C"` items and `#[repr(C)]` types — *`syn`-based; functions, repr(C) structs, integer enums, aliases, constants*
- [x] Emit C headers with include guards and Doxygen comments from Rust docs — *include guard set by the caller; `/** */` blocks on items and fields*
- [x] Enum/struct/callback/opaque-handle support — *opaque structs, function-pointer typedefs (`extern "C" fn`)*
- [x] CLI and `build.rs` entry points — *`tpt-dev-headers` binary (exit 0/1/2) and `build::write_header`, which writes only on change; `tests/cli.rs`*
- [x] Golden-file tests vs hand-written headers — *golden test against a generated-then-reviewed `api.h` (not hand-written); `BLESS=1` updates it*
- [ ] Compile generated headers with a C compiler in CI — *`c-headers` job in `ci.yml` (`gcc -std=c99 -Wall -Werror`). Not run locally, because no C compiler is installed here; ci.yml was checked only as YAML.*

### tpt-dev-ffi: zero-copy FFI state machine framework
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Typestate API for stateful C lifetimes — *`Closed`/`Open`/`Streaming` sessions; transitions consume the session (`session.rs`); compile-fail case
- [x] Ownership and borrow rules at the FFI boundary — *`with_view` gives a `for<'v>` zero-copy `View` that cannot escape the call or overlap a library call; compile-fail tests in `tests/ui/view_*.rs`. The unsafe contract sits in the backend's `view`.*
- [x] Handle/opaque-pointer helpers with validation — *`Handle<T>` is non-null and typed*
- [x] Integration with tpt-dev-headers and tpt-dev-automata — *header prototypes and the typestate machine are checked in `tests/c_library.rs`. The machine is hand-written and has no guards.*
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
- [x] Parse `Cargo.lock` and `cargo metadata --format-version 1` output — *strict JSON parser; metadata resolve graph with dev/build kinds (`--include-dev`/`--include-build`); `--metadata` flag; trimmed real fixture in `tests/metadata.rs`*
- [x] Circular dependency detection — *Tarjan's strongly connected components (`report.rs`)*
- [x] License violation detection (consistent with deny.toml policy) — *SPDX-style expressions checked against an allow list; the list is passed in, not read from deny.toml*
- [x] Bloat analysis (dep count, transitive size) — *transitive dependency counts per package; sizes in bytes are not measured*
- [x] CLI and report formats — *`tpt-dev-dep` binary with text output and exit codes; no JSON output*

### tpt-dev-doc: interactive, runnable documentation
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Extract examples from doc comments — *`extract` reads fenced blocks with rustdoc's tags and hidden lines; `doc_comments` takes `///` and `//!` text from source*
- [x] Embedded live REPL for running examples (decided: examples compiled to Wasm and run client-side in the browser, no server) — *`site::write_repl_page` writes the page; Wasm checked under Node 22; `tests/browser.rs` (opt-in, `TPT_DEV_DOC_BROWSER=1`) passes in headless Chrome 157 and Edge 157 on Windows: pass, trap, 5 s Worker timeout, CSP. Firefox and Safari not tested. Needs HTTP serving (file:// fails).*
- [x] Rustdoc integration (HTML injection) — *`tpt-dev-doc-site` binary (`site::run_post_doc_step`) runs after `cargo doc`; tested on a rustdoc-shaped temp tree. Build.rs output is overwritten by rustdoc, so it must be a post-step.*
- [x] Hosting-compatible output for docs.rs — *static only (decided); `site::write_examples_page` plus a test asserting no script, frame, form, image, link or URL. A build step must still call it.*
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
- [x] GitHub Actions example workflow — *`examples/ci/bench-gate.yml` runs the suite and gate; YAML parses. Not run on GitHub Actions; regenerate the baseline on the CI runner before enabling it.*
- [x] `no_std` cycle-counter mode — *`measure_with` and `CycleSource` in `cycles.rs` (no_std + alloc); the board supplies the counter; thumbv7em build passes*

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
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] ML-KEM (FIPS 203): 512/768/1024 — *`kem.rs`, with Keccak in `keccak.rs` and the ring and samplers in `poly.rs`*
- [x] ML-DSA (FIPS 204): 44/65/87 — *`dsa.rs`: NTT, rounding, sampling, sign and verify. Hedged and deterministic signing. Hashed (HashML-DSA) variants are not implemented.*
- [ ] Constant-time implementation, with verification (e.g. dudect-style tests) — *partial, host only: ML-KEM decapsulation shows no leak (|t| at most 1.84, `examples/timing.rs`). ML-DSA signing uses a fixed 64-attempt budget with masked checks; host |t| is at most 0.43 (was about 56), at roughly 9x the sign time. Not measured on Cortex-M. The reductions are now Montgomery (no `__aeabi_ldivmod` on thumbv7em); the `sample_in_ball` and keygen rejection loops remain.*
- [x] NIST KAT vectors pass — *first 3 records per set in `tests/fixtures`; all 3,000 records of the full NIST files pass (`TPT_KAT_DIR`, ignored test)*
- [ ] Zeroisation of secrets — *best effort: keys and secrets are overwritten on drop with `black_box`; copies made by the caller are not tracked*
- [x] Bring-your-own-RNG interface (no_std) — *`Rng` trait; `keygen` and `encapsulate` take it*
- [ ] Security review and a clear "not independently audited" notice — *notice written in the README; no independent review*
- [ ] Performance benchmarks on host and Cortex-M — *host done, Cortex-M open; host only (`examples/timing.rs`): keygen, encaps, decaps, sign and verify at all levels. Cortex-M not measured.*

### tpt-dev-zk: Rust-to-zero-knowledge circuit transpiler
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Circuit IR (decided: R1CS, with export to the circom `.r1cs` format; proof-system backends stay external) — *`Circuit` in `circuit.rs`; the field is BN254's scalar field (254-bit, Montgomery form)*
- [x] Rust function front end (proc-macro) for the supported subset — *`#[circuit]` in the companion crate `tpt-dev-zk-macros` (host-only, excluded from the bare-metal CI build) reads ordinary `i64` functions and writes a `Ring`-generic circuit version. Unsupported syntax is a compile error (trybuild cases). Generic functions over `Ring` are also traced.*
- [x] Transpile to circuit IR — *tracing: `+`, `-` add no constraint, a product of two wires adds one*
- [x] Witness generation — *values are computed while tracing; `Circuit::witness` gives them in format order*
- [x] Circuit export format — *iden3 `.r1cs` v1 and `.wtns` v2. snarkjs 0.7.4 reads the `.r1cs` as `bn-128` with the expected counts, and `wtns check` accepts the witness and rejects a corrupted one (see README)*
- [x] Tests comparing circuit output with native Rust execution — *`circuit::tests::the_circuit_agrees_with_native_execution`*
- [x] Document the supported language subset — *`README.md`, with limits*

### tpt-dev-tee: Trusted Execution Environment abstraction
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Common TEE trait set (secure memory, attestation, secure calls) — *`SecureMemory`, `Attestation`, `SecureCall`, shared `TeeError`*
- [ ] ARM TrustZone backend (Cortex-M NS/S transitions) — *partial: `trustzone` models the NS/S partition (deny by default) and the gate (registered entries only, no nesting), tested on the host. Not done: programming the SAU/IDAU, secure gateway veneers, and any hardware test.*
- [ ] Intel TDX backend — *not started: needs Intel's guest interface, which cannot be built or tested in this environment*
- [x] Mock TEE for host tests — *`mock::MockTee`: enforces world isolation and service calls; its attestation tag is explicitly not a cryptographic MAC*
- [ ] Safety review of secure/non-secure boundary code — *the README lists the review points for the modelled rules; the review of hardware boundary code waits for a backend*

### tpt-dev-compose: Wasm Component Model linker
- [x] S — *0.1.0 publish deferred to Phase 7*
- [x] Wasm binary/component parser (pure Rust) — *core modules in `module.rs`; components in `component.rs`: sections (id, offset, size, vector item counts), custom names, embedded core modules and nested components to depth 16. Vector item contents are not decoded.*
- [x] Component Model types and canonical ABI — *all listed value kinds decoded; layouts hand-derived and checked by hand; canon memory option not checked*
- [ ] Instance linking and composition at runtime — *partial: `Registry::link` plans core-module and component-level bindings (kind-tagged `ComponentBinding`, `component_order`). Core modules instantiate and run in a defined integer-only subset (`Runtime`); memories, tables, floats and component instantiation are not supported.*
- [x] Import/export resolution and validation — *index bounds exact in every space (canonical section decoded, outer aliases checked); canon memory-option index not checked*
- [x] Conformance tests against sample components — *`tests/component.rs` checks ids, offsets, sizes and counts against `wasm-tools dump` for three fixtures (one nested); `wasm-tools validate` accepts them*

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

- [x] Cross-crate dependency and version audit — *every path dependency uses version `0.0.0`, matching the workspace*
- [x] Final `cargo deny check` across the whole workspace — *advisories, bans, licenses and sources all pass*
- [x] Confirm every crate builds on `thumbv7em-none-eabihf` and passes loom where applicable — *bare-metal build passes with CI's exclusions; loom passes for async (2 tests), lock (3) and trace (2, about 90 s in release)*
- [x] Confirm every public function documented with example or README include — *`missing_docs` is enforced by clippy `-D warnings`, and `cargo doc` passes with warnings as errors; not every item has an example* — *the lint reports zero; the two `ignore`d macro examples noted under the example item above.*
- [ ] Publish 0.x in dependency order (decided: `cargo-release` resolves it; layers 0, 1, 2, 3, then Phase 6 crates) — *not done: publishing needs the user's go-ahead*
- [ ] First release via `cargo release` for all 30 crates — *not done: publishing needs the user's go-ahead*
- [ ] Verify the docs.rs builds for all 30 crates — *local `cargo doc --workspace --all-features` passes with warnings as errors; docs.rs itself has not been checked*
- [x] Final workspace `README.md` ecosystem overview with a crate index
- [x] Semantic versioning policy documented — *`VERSIONING.md`*
- [x] 1.0 API-stability review per crate (1.0 signals stability) — *`docs/api-review.md`; findings include no `#[non_exhaustive]` public types and required trait methods.*
- [x] Ongoing: patch release process via `cargo release patch --workspace --execute` — *documented in `VERSIONING.md`. The first-release dry run has a known blocker: path dependencies still carry `0.0.0`.*

---

## Phase 8: Loose ends & adoption tooling

*Added 2026-11-20 after a project review. No `TODO`/`todo!` markers exist in code; these are the real stubs, plus tooling to make adoption easier.*

### Loose ends (code only)
- [x] Bench gate placeholders: write a real bench example in `tpt-dev-bench/examples/` that emits the `name mean stddev n` baseline, commit a sample baseline, and replace the `PLACEHOLDER-*` values in `examples/ci/bench-gate.yml` (also closes the Phase 5 bench item)
- [x] Remove stray `wip/` dirs after diffing against `tests/ui/`: `automata`, `codec`, `dsl-macros`, `lock`, `mock` (all five are present; the `codec` one differs from `tests/ui/big_unaligned.stderr`, so check before deleting) — done: the codec `wip/` file differed only in compiler wording, so the live `tests/ui` fixtures were kept.
- [x] Add runnable examples to public items until `tpt-dev-lint` `doc-example` reports zero (460 findings). Start with solver, contract, automata, trace, mem (closes the Phase 7 doc item) — *`tpt-dev-lint` `doc-example` reports zero across the workspace. Two macro-crate examples (`tpt-dev-mock-macros`, `tpt-dev-zk-macros`) are `ignore`d because a runnable one would need a dev-dependency cycle; their behaviour is tested in the runtime crates.*
- [x] pqc: replace the `i64 % q` reductions with Barrett or Montgomery (removes `__aeabi_ldivmod` on thumbv7em), then re-run `examples/timing.rs` — *Montgomery reduction; output bit-identical; no `__aeabi_ldivmod` in the thumbv7em build. Timing re-run, with no before/after comparison.*
- [x] bench: choose the t critical value from degrees of freedom (`stats.rs`); today it is about 2 — done: `t_critical_95` uses the Welch degrees of freedom; `--critical-t` overrides it.
- [x] dep: JSON output, parenthesised licence expressions, licences read from `cargo metadata` — done: `--json`, parenthesised expressions, licences from `cargo metadata`.
- [x] solver: hand-written QF_LRA corpus (20 to 30 `.smt2` files with known answers) in `tests/corpus/`, checked by a test — done: 27 hand-checked files, expectations on `; expect:` lines (`tests/corpus.rs`); the larger SMT-LIB benchmark set is still not available offline.
- [x] Fix the README crate count (says 30; the workspace has more with the companion macro crates)
- [ ] Optional, larger: `#[derive]` front end for codec — *the other items from this list are done: incremental `PackratStream`, early `return` in contract, automata `machine!` macro, mock associated types.*
- [x] Optional, larger: runtime instance linking and execution in compose — *core modules only, integer subset; components are still plan-only.*

### Adoption tooling
- [x] Facade crate `tpt-dev`: one dependency, a feature per group (`verify`, `rt`, `ffi`, `security`, `edge`), `std` forwarding, a `prelude` module, `no_std` by default
- [x] `cargo-tpt` CLI (new host-only crate `tpt-dev-cli`): `lint`, `dep`, `headers`, `doc`, `bench-gate`, and `check` (lint, dep and licence policy, fmt, clippy; non-zero exit on failure). Reuse the library entry points of the existing tools
- [x] `docs/getting-started.md`
- [x] `docs/which-crate.md` (problem to crate table)
- [x] Feature and `no_std` matrix (no_std, alloc, host-only, proc-macro per crate)
- [x] Status column (`wip`, `alpha`, `stable`) in the README crate table
- [x] Replace the copy-pasted check commands in README and CONTRIBUTING with `cargo tpt check` — done for README and CI; CONTRIBUTING had no copied commands.
- [x] Example: verified contract (`#[requires]`/`#[ensures]` proof plus a solver-seeded fuzz of the same precondition) — done without a fuzz step: the `fuzz` API was not wired in; the example uses the contract and solver only.
- [x] Example: automata state machine driving an FFI typestate wrapper and a generated C header — done with a Rust-source counter standing in for C (no C compiler here).
- [x] Example: record an async run, replay it bit-for-bit, and export spans through sink — done with the trace collector; exporting through `sink` is not done.
- [x] `CHANGELOG.md` (CONTRIBUTING already promises MSRV bumps are noted there)
- [ ] `SECURITY.md` (removed at the maintainer's request; re-add when a reporting route is chosen) (reporting route; pqc, zk and tee carry a "not audited" notice)
- [x] Issue templates (design-first) and a PR template — done for issues; no PR template, because CONTRIBUTING does not accept pull requests.
- [x] `.devcontainer` with the `thumbv7em-none-eabihf` target and gcc (also lets the C-headers job run locally)
- [x] `cargo-generate` templates: `no-std-firmware` and `verified-lib`
- [x] CI: publish rustdoc to GitHub Pages (nothing deploys docs today) — workflow added; enable Pages in repository settings to verify.

### Innovative cross-crate features
- [x] contract + fuzz + test: write a failed proof's counterexample out as a regression test — *example in `tpt-dev-contract`; generates `tests/regression_step_down.rs`.*
- [x] automata + ffi + headers: one machine definition emits the typestate guard, a C header transition table, and a solver-checked "no illegal transition" proof — *example in `tpt-dev-ffi`. The "no illegal transition" check is exhaustive over the finite state and event sets, not a solver proof (the machine has no guards).*
- [x] bench + profiler: real-time budget gate in CI — *example in `tpt-dev-profiler`; CI template included. Regenerate the baseline on the CI runner before enabling.*
- [x] replay + automata: check a replayed run against a state machine spec — *example in `tpt-dev-replay`.*
- [x] ast + dsl + codegen + codec: one schema definition generates codec, parser, header and tests — *example in `tpt-dev-codec`. The parser uses dsl's `PackratStream`, not `ast` directly.*

### Needs external action (not code-only)
- [ ] Hardware: WCET (async), Cortex-M benchmarks and constant-time checks (pqc), profiler overhead on target, TrustZone SAU/IDAU and veneers, TDX
- [ ] Full NIST KAT run (`TPT_KAT_DIR`) and an independent security review of pqc
- [ ] Run the `c-headers` CI job and the full pipeline on GitHub Actions; re-run the `cargo release` dry run on a committed tree
- [x] Compile-time benchmark vs mockall (needs network)

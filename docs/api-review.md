# 1.0 API-stability review

This review reads each crate's public surface and asks whether it can be frozen for 1.0. It is a reading of the code at the workspace's `0.0.0` version, not a consumer test. Public items were found by grepping `pub` declarations in each `src/` tree, so treat the counts as approximate. Status comes from the crate table in [README.md](../README.md).

Verdict scale:

- **Ready after small changes:** the surface is coherent; the listed items should be fixed before 1.0.
- **Needs changes:** the surface has design decisions still open, or a trait or type that will break users on the next change.
- **Not ready:** the crate is incomplete, unaudited, or its behaviour is still moving.

## Headline findings

1. **No public type is `#[non_exhaustive]`.** The workspace has about 67 public enums (21 of them error types), about 24 public traits and about 200 public structs. Adding a variant to any of these is a breaking change today. Before 1.0, mark each enum that may grow, and each config struct that may gain a field, `#[non_exhaustive]`, or move construction behind a builder.
2. **Public config structs expose their fields.** Examples are `bench::Policy`, `solver::Budget`, `test::Config`, `fuzz::Config`, `dep::Policy`, and `paged::Stats`. Adding a field breaks every struct literal in user code.
3. **Public traits with required methods are breaking to extend.** `ast::Lexer` and `ast::Visitor`, `dispatch::Device`, `ffi::Backend`, `lint::Rule`, `sink::Sink`, `test::Generator`, `trace::{Clock, IdSource, Sink}`, `tee::{SecureMemory, Attestation, SecureCall}`, `zk::Ring` and `pqc::Rng` all need a decision on whether implementors are expected outside the crate. Sealing the ones that are not, or giving the others default methods, should happen before 1.0.
4. **Several crates define wire or on-disk formats that are not yet versioned.** These are the 16-byte replay log (`replay::Event`), the codec bit layout (`codec::Layout`), the `.r1cs`/`.wtns` output (`zk`), the baseline text format (`bench::Baseline`), the DRUP-style proof output (`solver::Proof`), the seed-to-case mapping in `test` (`test::Seed`, which reproduces failures), and the timer register map in `sim`. A 1.0 needs either a stated stability promise for each format or a version field.
5. **Behaviour changes are already happening in the `0.0.0` surface.** `pqc` signing changed to fixed work in `5bf801d`, and `bench` changed `Policy::critical_t` to `Option<f64>`. Both are noted in [CHANGELOG.md](../CHANGELOG.md) or the commit history. `pqc` and `compose` are still moving (see below), so they cannot be frozen yet.
6. **The facade re-exports every crate.** `tpt-dev::prelude` and the feature groups mean a breaking change in any crate is also a breaking change to `tpt-dev`. The facade should be reviewed last.
7. **Crate counts disagree.** [README.md](../README.md) says 33 crates, [todo.md](../todo.md) says 30, and `crates/` has 36 directories. The README counts four proc-macro companions separately from the crates in its table. The count should be stated once, from the directory list, before 1.0.

## Per-crate review

### Facade and tooling

#### tpt-dev (facade)
- **Status:** alpha
- **Public surface:** `prelude` module re-exporting 28 crate modules; feature groups `verify`, `contract`, `rt`, `ffi`, `security`, `edge`, `lang`, `bench`, `tools`, `full`, `std`.
- **Verdict:** needs changes. Review last, after the crates it re-exports.
- **Breaking-change candidates:** the prelude contents grow with every new crate; the feature groups are a public promise and should be listed in the docs; `std` forwarding must stay additive.

#### tpt-dev-cli
- **Status:** alpha
- **Public surface:** no library items. The `cargo tpt` binary has subcommands `check`, `lint`, `dep`, `headers`, `doc`, `bench-gate`.
- **Verdict:** ready after small changes. The CLI flags and exit codes are the interface, so they need a stated policy.
- **Breaking-change candidates:** exit codes (0/1/2 in `lint` and `dep`), flag names, and output formats (`dep --json` is a schema).

#### Companion proc-macro crates
- `tpt-dev-contract` (`requires`, `ensures`), `tpt-dev-dsl-macros` (`dsl`), `tpt-dev-mock-macros` (`mock`), `tpt-dev-zk-macros` (`circuit`). Each exports one or two macros. Their generated code is part of the API, so a change to generated output is breaking. The two with their own sections are covered under their owning crates below.

### Formal verification

#### tpt-dev-solver
- **Status:** alpha
- **Public surface:** about 99 public items: `Solver`, `Session` (SMT-LIB script), `Arena`, `FixedArena`, `Formula`, `Linear`, `Var`, `Rational`, `Budget` (pub fields), `Status`, `Proof`, `ProofStep`, `check_proof`, `check_farkas`, `Farkas`, `ScriptError`, `Unknown`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Status` and `ProofStep` need `#[non_exhaustive]`; `Budget` fields need a builder or non-exhaustive; the proof format is part of the API and must be versioned; `Rational` overflow semantics (returns `Unknown`) must be documented as stable.

#### tpt-dev-contract
- **Status:** alpha
- **Public surface:** two proc-macros, `requires` and `ensures`. Subset limits are in the README.
- **Verdict:** ready after small changes. The supported subset is the real contract, so the README subset must be frozen with the version.
- **Breaking-change candidates:** the proved-vs-checked behaviour (`ensures` proved statically or checked at run time), the error text of compile-time counterexamples, and the `old()` rules.

#### tpt-dev-fuzz
- **Status:** alpha
- **Public surface:** `Fuzzer`, `Config` (pub fields), `Coverage`, `Outcome`, `Found`, `minimise`, `solver_seeds`, `SeedError`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Config` fields (`iterations`, `seed`, ...) need non-exhaustive or a builder; `Outcome` and `SeedError` need `#[non_exhaustive]`; the seed-to-input mapping is a reproducibility promise and must be stated.

#### tpt-dev-test
- **Status:** alpha
- **Public surface:** about 63 public items: `check`, `replay`, `prove`, `infer`, `find_invariant`, `Generator` (trait), `IntRange`, `Bools`, `VecOf`, `Pair`, `ConstrainedGen`, `Config` (pub fields), `Seed`, `Rng`, `Outcome`, `Verdict`, `Failure`, `SetupError`, `SpecError`, `TransitionSystem`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Generator` is a public trait with required methods; `Seed` and `Rng` output sequences are a reproducibility promise; `Config` and `SearchConfig` need non-exhaustive; the invariant-search limits in the README must match the API.

#### tpt-dev-automata
- **Status:** alpha
- **Public surface:** about 54 public items: `Machine`, `Spec`, `Atom`, `Transition`, `StateId`, `Analysis`, `Deadlock`, `DeadlockKind`, `Verdict`, `Cmp`, `Error`, `const_check::{assert_deadlock_free, check_deadlock_free}`, and the `MAX_*` constants (64 states, 4 variables).
- **Verdict:** needs changes.
- **Breaking-change candidates:** the `MAX_*` limits are part of the API, so raising them is a minor change and lowering them is breaking; `DeadlockKind`, `Verdict`, `Cmp` and `Error` need non-exhaustive; the const-check function signatures are fixed by the const-evaluation guarantee.

### FFI, AST and codecs

#### tpt-dev-ast
- **Status:** alpha
- **Public surface:** `Event`, `Fault`, `Step`, `Token`, `ParseError`, `DepthExceeded`, `Feed`, `Stack`, `Stream`, `Parser`, `Visitor` (trait), `Lexer` (trait), `JsonLexer`, `Lex`, `lex`, `parse_all`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Event`, `Fault`, `Step`, `Token` and `ParseError` need non-exhaustive; `Lexer` and `Visitor` need decisions on default methods; `JsonLexer` is an example grammar and may need to move or be marked as such.

#### tpt-dev-headers
- **Status:** alpha
- **Public surface:** `generate`, `write_header`, `rerun_if_changed`, `build` module, `Options` (pub fields), `Error`, `BuildError`.
- **Verdict:** ready after small changes.
- **Breaking-change candidates:** the generated C output format (golden tests pin it); `Options` needs non-exhaustive; `Error` and `BuildError` should be merged or marked.

#### tpt-dev-ffi
- **Status:** alpha
- **Public surface:** typestate types `Closed`, `Open`, `Streaming`, `Failed`, `Session`, `View`, `Handle`, `Code`, and trait `Backend`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Backend` is a public trait with `unsafe`-contract methods; adding a method breaks every backend; the `View` lifetime rules are a soundness contract and must be frozen with a test; `Code(pub i32)` exposes its field, so C status codes cannot gain a checked representation without breaking users.

#### tpt-dev-codec
- **Status:** alpha
- **Public surface:** `Layout`, `Field` (pub fields), `Endian`, `CodecError`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** the bit layout is the wire format, so its encoding must be fixed by tests; `CodecError` and `Endian` need non-exhaustive; `Field` pub fields need a constructor.

#### tpt-dev-codegen
- **Status:** alpha
- **Public surface:** `Writer`, `Error`, `is_ident`, `string_literal`, `write_if_changed`, `rerun_if_changed`, `rerun_if_env_changed`, `build` module.
- **Verdict:** ready after small changes. Small surface.
- **Breaking-change candidates:** `Error` needs non-exhaustive; `Writer` output layout is pinned by the comparison example.

### Real-time and concurrency

#### tpt-dev-async
- **Status:** alpha
- **Public surface:** `Executor`, `Full`, `Stats` (pub fields), `Delay`.
- **Verdict:** needs changes. Executor limits (32 slots, round bound) are part of the contract.
- **Breaking-change candidates:** the slot limit, `Stats` fields, `Full` error shape; the WCET bound is unmeasured (todo item open), so the bound cannot be frozen.

#### tpt-dev-trace
- **Status:** alpha
- **Public surface:** `Collector`, `Tracer`, `SpanGuard`, `SpanId`, `TraceId`, `Context`, `Record` (pub fields), `Kind`, `Ring`, `Sink` (trait), `Clock` (trait), `ManualClock`, `IdSource` (trait), `AtomicIds`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Sink`, `Clock` and `IdSource` need a decision on required methods; `Record` fields need a constructor; `Kind` needs non-exhaustive; the record layout is exported to `replay` and `sink`.

#### tpt-dev-lock
- **Status:** alpha
- **Public surface:** `Mutex`, `Spsc`, `TripleBuffer`, `Producer`, `Consumer`, `Reader`, `Writer`, `Token`, the order modules `order`/`L`/`Above`/`Nil`/`Succ`, and the lock-level types.
- **Verdict:** needs changes.
- **Breaking-change candidates:** the lock-level type encoding (`L`, `Above`, `Nil`, `Succ`) is a public design choice and a change to it breaks every user's lock order; the loom and trybuild tests pin the behaviour; `Spsc` capacity is a const parameter and its bounds are part of the API.

#### tpt-dev-mem
- **Status:** alpha
- **Public surface:** `Reg`, `Field<OFFSET, WIDTH>` (const generics), access types `RO`, `RW`, `W`, `WO`, `W1C`, `Readable`/`Writable` (sealed), `Word` (sealed).
- **Verdict:** ready after small changes. The smallest surface of the concurrency group and its traits are already sealed.
- **Breaking-change candidates:** the access-mode type names (`RO`, `RW`, `W`, `WO`, `W1C`) are part of the API; the `Field` const-generic parameters are part of the type; `Word` is implemented for `u8`–`u64`, and adding `u128` would be additive.

#### tpt-dev-replay
- **Status:** alpha
- **Public surface:** `Event`, `Log`, `Ring`, `Full`, `Session`, `Progress`, `Divergence`, `step`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** the 16-byte event format is a wire format and must be versioned before 1.0; `Event` and `Progress` need non-exhaustive; `Divergence` fields need a stated shape.

### Performance and profiling

#### tpt-dev-profiler
- **Status:** alpha
- **Public surface:** `Sampler<N, D>` (const generics), `Symbols`, `Report`, `Counts` (pub fields), `UNKNOWN`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** the const-generic parameters become part of the type; `Counts` and `Report` fields need non-exhaustive or accessors; the folded-stacks output format is a stable promise only if documented.

#### tpt-dev-bench
- **Status:** alpha
- **Public surface:** `measure`, `measure_with`, `judge`, `judge_entry`, `compare`, `summarise`, `t_critical_95`, `trim_outliers`, `Baseline`, `Entry`, `Policy` (pub fields), `Verdict`, `Comparison`, `Summary`, `CycleSource` (trait), `ManualCycles`, `ParseError`, `harness` module.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Policy::critical_t` already changed type in `0.0.0` (noted in CHANGELOG); `Policy`, `Comparison` (gained `df`) and `Summary` need non-exhaustive; the baseline text format is a stored-file format and must be versioned; `CycleSource` needs a decision on required methods.

#### tpt-dev-dispatch
- **Status:** alpha
- **Public surface:** `Device` (trait), `CpuDevice`, `MockAccelerator`, `Kind`, `Op`, `Graph`, `TaskId`, `GraphError`, `Schedule`, `Assignment`, `ScheduleError`, `schedule`, `execute`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Device` needs required methods reviewed; `Kind` and `Op` need non-exhaustive (adding GPU ops is expected); the cost model is a heuristic and its outputs must not be frozen; `MockAccelerator` is test-only and should be behind a feature or a `#[doc(hidden)]` module.

### Developer tooling

#### tpt-dev-doc
- **Status:** alpha
- **Public surface:** about 34 public items: `extract`, `doc_comments`, `Example`, `Mode`, `compile`, `page`, `escape`, `example_section`, `inject_before`, `inject_into_rustdoc`, `run_post_doc_step`, `PostDocReport`, `Injection`, `write_examples_page`, `write_repl_page`, `wasm_source`, consts `REPL_FILE_NAME` and `RUNNER_JS`, modules `site` and `wasm`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `RUNNER_JS` and the generated page contents are shipped code, so their change is a behaviour change; the sandbox review depends on the page's content and must be re-run when it changes; `Example` and `Mode` need non-exhaustive.

#### tpt-dev-lint
- **Status:** alpha
- **Public surface:** `Linter`, `Rule` (trait), `Finding`, `Diagnostic`, `Severity`, `Config`, `ConfigError`, `default_rules`, `has_errors`, `DocExample`, `NoUnwrap`, `NoTodoMacros`, `SafetyComment`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Rule` is a public trait and needs a stated extension policy; rule names are part of the config file format (`rule = allow|warn|deny`) and renaming one breaks users; `Severity` needs non-exhaustive; the `doc-example` rule produces 460 findings, so its default severity may need to change before 1.0.

#### tpt-dev-dep
- **Status:** alpha
- **Public surface:** `analyse`, `Report`, `Graph`, `Package`, `Dependency`, `DepKind`, `Scope`, `Policy` (pub fields), `license_allowed`, `parse_lock`, `parse_metadata`, `parse_metadata_licenses`, `parse_json`, `Value`, `JsonError`, `MAX_DEPTH`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** the `--json` output schema is a public format; `Policy` needs non-exhaustive; `Value` and `JsonError` are a JSON parser exported as API, which is a large promise and should be reviewed (a `pub` parser is a commitment); `MAX_DEPTH` is a limit.

#### tpt-dev-mock
- **Status:** alpha
- **Public surface:** `Recorder`, `ExpectBuilder`, re-export of `mock` macro.
- **Verdict:** needs changes. The macro's generated code shape is part of the API.
- **Breaking-change candidates:** the expectation builder method names (`with`, `times`, `returns`, `returns_with`, `verify`, `calls`); the macro output for trait methods with associated types (listed as an optional todo item).

#### tpt-dev-mock-macros (companion)
- **Status:** not in README table (companion)
- **Public surface:** `mock` attribute macro.
- **Verdict:** needs changes. Generated code is public.

#### tpt-dev-dsl
- **Status:** alpha
- **Public surface:** `Grammar`, `Rule`, `Expr`, `Tree`, `Expected`, `ParseError`, `PackratStream`, `GrammarLexer`, `StreamError`, `Memo`, `MemoFull`, `PackratError`, `MAX_DEPTH`, re-export of `dsl`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Expr` is the grammar AST, and adding a variant breaks every exhaustive match, so it needs non-exhaustive; `PackratStream` is not yet incremental (todo open), so its buffering behaviour is not stable; error spans from the macro are part of the diagnostic contract.

#### tpt-dev-dsl-macros (companion)
- **Status:** not in README table (companion)
- **Public surface:** `dsl!` proc-macro.
- **Verdict:** needs changes. Error spans and the generated parser shape are public.

### Platform and hardware

#### tpt-dev-sim
- **Status:** alpha
- **Public surface:** `Kernel<N>`, `Component` (trait), `Ctx`, `SignalId`, `Timer`, `Deadline`, `MinGap`, `Vcd`, `SimError`, `Failure`, register constants `COUNT`, `CTRL`, `CTRL_ENABLE`, `CTRL_IRQ_ENABLE`, `PERIOD`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** the timer register map constants are a hardware-model contract; `Component` is a public trait with `tick`; `SimError` and `Failure` need non-exhaustive; `Vcd` output format must be stated.

#### tpt-dev-sink
- **Status:** alpha
- **Public surface:** `Router`, `Rule`, `RuleError`, `Envelope`, `Routed`, `Lane`, `LaneStats`, `Busy`, `Policy`, `Sink` (trait), `TraceSink`.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Sink` trait with required methods; `Policy` (back-pressure and drop) needs non-exhaustive; `Envelope` layout is exported; `LaneStats` fields.

#### tpt-dev-pqc
- **Status:** wip
- **Public surface:** `MlKem`, `MlDsa`, `Parameters` (trait), `Rng` (trait), key and signature types (`EncapsulationKey`, `DecapsulationKey`, `Ciphertext`, `SharedSecret`, `SigningKey`, `VerifyingKey`, `Signature`), `keygen`, `keygen_from_seed`, `encapsulate`, `encapsulate_from_seed`, `decapsulate`, `sign`, `sign_deterministic`, `verify`, `KemError`, `DsaError`, `SHARED_SECRET_LEN`.
- **Verdict:** not ready. Constant-time work is partial (see [todo.md](../todo.md) Phase 6), the Cortex-M numbers are missing, and there is no independent audit.
- **Breaking-change candidates:** `Parameters` and `Rng` are public traits, so adding a method breaks implementors; signing behaviour changed in `5bf801d`, so the function's output and timing are not stable yet; `KemError` and `DsaError` need non-exhaustive; the hedged vs deterministic signing split needs a stated default; the lack of an audit must be in the crate docs.

#### tpt-dev-tee
- **Status:** wip
- **Public surface:** `SecureMemory`, `Attestation`, `SecureCall` (traits), `TeeError`, `World`, `Security`, `Region`, `Partition`, `Gate`, `Report`, `Measurement`, `measure`, `DIGEST_LEN`, `MockTee`, modules `mock` and `trustzone`, type `Handler`.
- **Verdict:** not ready. The TrustZone backend is a model only and the TDX backend is not started.
- **Breaking-change candidates:** the three traits will change when real backends arrive; `TeeError`, `World` and `Security` need non-exhaustive; the `Measurement` and `Report` formats are not stable; `MockTee` must not be the reference for security claims (its attestation is not a MAC, as the crate docs say).

#### tpt-dev-compose
- **Status:** wip
- **Public surface:** about 156 public items, the largest in the workspace: `module`, `component`, `types`, `link` modules; `parse`, `flatten`, `func_abi`, `layout`, `tuple_layout`; `Registry`/`Binding`/`ComponentBinding`/`LinkError`; many enums mirroring the Wasm spec (`ValType`, `ValueType`, `TypeDef`, `ExternKind`, `ExportKind`, `Canon`, `CanonOption`, `Decl`, `Alias`, and others); `MAX_DEPTH`, `MAX_FLAT_PARAMS`, `MAX_FLAT_RESULTS`.
- **Verdict:** not ready. Linking only plans, and execution is not implemented (todo item open).
- **Breaking-change candidates:** the Wasm-spec enums grow whenever the spec does, so they need non-exhaustive; the `link` planning API will change when instantiation lands; the `MAX_*` limits are fixed by the spec and the crate's own bounds; around 75 public fields need a decision.

#### tpt-dev-paged
- **Status:** alpha
- **Public surface:** `Pager`, `Stats` (pub fields), `Error`.
- **Verdict:** ready after small changes.
- **Breaking-change candidates:** `Error` and `Stats` need non-exhaustive; the `MAXP` per-sequence limit is part of the contract; the eviction policy (LRU of whole sequences) is a behaviour promise.

#### tpt-dev-zk
- **Status:** alpha
- **Public surface:** `Circuit`, `Wire`, `Ring` (trait), `F` (field element), `MODULUS_LE`, re-export of `circuit` macro.
- **Verdict:** needs changes.
- **Breaking-change candidates:** `Ring` is public and generic, so adding a method breaks implementors; `F` and `MODULUS_LE` fix the BN254 field, so changing curves is breaking; the `.r1cs` v1 and `.wtns` v2 export format must be pinned; the supported-subset README is the contract.

#### tpt-dev-zk-macros (companion)
- **Status:** not in README table (companion)
- **Public surface:** `circuit` attribute macro.
- **Verdict:** needs changes. The supported syntax set and the error messages are public.

## Next steps before 1.0

- Add `#[non_exhaustive]` to the enums and config structs listed above, in one pass per crate, with a CHANGELOG entry.
- Seal or give default methods to the public traits, following the notes above.
- Decide which formats are stable (replay log, codec layout, baseline, proof, `.r1cs`, timer register map), and add a version field or a stated promise for each.
- Reconcile the crate count across README, todo and the workspace.
- Re-run this review after the `pqc`, `tee`, and `compose` work lands. Those three are the crates whose verdict depends on hardware and audit work.

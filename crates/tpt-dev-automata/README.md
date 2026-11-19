# tpt-dev-automata

State machine logic verifier. It finds states that can never be left, and proves deadlock freedom where the guards are arithmetic.

- **Machines with guards:** states and event transitions. Each guard is a linear formula over real variables, built with `tpt-dev-solver`.
- **Reachability:** a breadth-first search from the initial state. Unreachable states are truly unreachable.
- **Deadlock analysis:** a reachable, non-final state is stuck when it has no outgoing transitions, or when its guards can all be false together. The solver decides the second case and gives a witness valuation.
- **Counterexample traces:** each deadlock comes with the shortest event sequence from the initial state.

## Example

```rust
use tpt_dev_automata::{DeadlockKind, Linear, Machine, Rational};

let mut m = Machine::new();
let x = m.declare("x");
let wait = m.state("wait");
let done = m.state("done");
m.mark_final(done);

let gt0 = m.arena_mut().gt(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
m.transition(wait, done, "go", Some(gt0)).unwrap();

// `wait` gets stuck when x <= 0.
let report = m.analyze(wait).unwrap();
assert!(matches!(report.deadlocks[0].kind, DeadlockKind::GuardsIncomplete { .. }));
```

## Compile-time check

`const_check` can reject a deadlocking machine during compilation:

```rust,ignore
use tpt_dev_automata::const_check::{assert_deadlock_free, Spec};

const SPEC: Spec<'_> = /* states, variables, edges written as data */;
const _: () = assert_deadlock_free(&SPEC); // a deadlock fails the build
```

The compile-fail cases are in `tests/ui/`.

## Limits

- **Guards are not used for reachability.** A state reported reachable might not be reachable with the real variable values. The deadlock witness is a valid valuation, but that valuation might not be reachable.
- **Compile-time checking covers only a subset.** `const_check` checks deadlock freedom in a `const` context. It takes machines whose guards are conjunctions of `x <op> c` atoms with integer `c`, written as plain data. Other guards, including rational thresholds and multi-variable linear guards, need the run-time `analyze`. Missing transitions in the `Spec` are not detected, so the spec must match the real machine.
- **Single initial state.** Analysis starts from one state.
- **Overflow is reported, not hidden.** If the solver cannot decide a guard, `analyze` returns `Error::Unknown`.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

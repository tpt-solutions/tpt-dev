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

## Limits

- **Guards are not used for reachability.** A state reported reachable might not be reachable with the real variable values. The deadlock witness is a valid valuation, but that valuation might not be reachable.
- **Compile-time proof is not done.** The analysis runs at run time. A compile-time check needs a proc macro, which is not written yet, so the compile-fail tests for this crate are also pending.
- **Single initial state.** Analysis starts from one state.
- **Overflow is reported, not hidden.** If the solver cannot decide a guard, `analyze` returns `Error::Unknown`.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

# tpt-dev-automata

State machine logic verifier that proves deadlock freedom at compile time.

## Overview

`tpt-dev-automata` analyses finite state machines whose transitions carry arithmetic guards. It finds states that can never be left, and it proves deadlock freedom where the guards together cover every input. The guard logic is decided by [`tpt-dev-solver`](https://docs.rs/tpt-dev-solver), so a stuck state comes with a concrete valuation of the variables and the shortest event sequence that reaches it.

There are two ways to use it. `Machine` is the general, run-time API: guards are any linear formula the solver can express. `const_check` is a smaller guard language that `const fn` can evaluate, so a machine written as data can fail the build when it can deadlock. The crate is `no_std` with `alloc`.

## Features

- **Machines with guards:** states and event transitions. Each guard is a linear formula over real variables, built with `tpt-dev-solver` types that this crate re-exports.
- **Reachability:** a breadth-first search from the initial state. States reported unreachable are truly unreachable.
- **Deadlock analysis:** a reachable, non-final state is stuck when it has no outgoing transitions, or when its guards can all be false together. The solver decides the second case and gives a witness valuation.
- **Macro front end:** `machine!` declares variables, states, final states and guarded transitions in one statement. Each guard is a comparison of a variable with an integer.
- **Counterexample traces:** each deadlock comes with the shortest event sequence from the initial state.
- **Compile-time check:** `const_check::assert_deadlock_free` runs in a `const` context with no allocation and fails compilation when a machine in its guard subset can deadlock.

## Installation

```sh
cargo add tpt-dev-automata
```

The crate is `#![no_std]` and needs `alloc`. It depends on `tpt-dev-solver`. `const_check` itself uses fixed-size arrays and does not allocate.

| Feature | Enables | Default |
| ------- | ------- | ------- |
| `std`   | Links `std`. The library has no `std`-only code today, so the flag changes nothing in the public API. | off |

## Quick start

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

## Macro front end

`machine!` builds the same `Machine` as the quick start, with less code. Each guard is one comparison of a declared variable with an integer: `<`, `<=`, `>`, `>=`, `==` or `!=`. A transition with no `when` clause is always enabled.

```rust
use tpt_dev_automata::machine;

machine! {
    m {
        vars: [x];
        states: [idle, busy];
        finals: [idle];
        transitions: [
            idle -> busy : "start" when x > 0;
            busy -> idle : "stop" when x <= 0;
            busy -> busy : "tick" when x > 0;
        ];
    }
}

// The statement binds `m`, `x`, `idle` and `busy` in the surrounding scope.
assert!(m.analyze(idle).unwrap().is_deadlock_free());
```

The macro expands to calls on the builder API, so `m` can be analysed like any other `Machine`. A guard whose constant does not fit the solver's range becomes a guard that is never enabled. That can only add reported deadlocks, never hide one. Guards that need more than one variable or a non-integer constant still need the builder API.

## Compile-time check

`const_check` can reject a deadlocking machine during compilation. A guard is a conjunction of atoms `x <op> c` with integer `c`:

```rust
use tpt_dev_automata::const_check::{assert_deadlock_free, Atom, Cmp, Edge, Spec};

const POS: &[Atom] = &[Atom::new(0, Cmp::Gt, 0)];
const NONPOS: &[Atom] = &[Atom::new(0, Cmp::Le, 0)];
const EDGES: &[Edge<'_>] = &[
    Edge { from: 0, to: 1, guard: POS },
    Edge { from: 0, to: 0, guard: NONPOS },
];
// State 0 waits until x > 0. State 1 is final.
const SPEC: Spec<'_> = Spec { states: 2, vars: 1, initial: 0, finals: &[1], edges: EDGES };

// Fails the build if a reachable, non-final state can get stuck.
const _: () = assert_deadlock_free(&SPEC);
```

Use `check_deadlock_free` to get a `Verdict` instead of a panic. The verdicts are `DeadlockFree`, `NoTransitions`, `GuardsIncomplete`, `OutOfRange` and `TooLarge`. The compile-fail cases are in `tests/ui/`.

## Design notes and guarantees

- **Guards are decided exactly.** The run-time analysis asks the solver whether every outgoing guard can be false at once, over exact rationals.
- **Const check method.** Each variable's real line is split into points and open gaps at the guard thresholds, and one representative per cell is checked on every combination. That decides whether the guards cover all valuations in the subset.
- **The two checks agree.** `tests/const_check.rs` compares the const check with the run-time analysis on random machines.
- **Bounded const check.** Limits are `MAX_STATES` (64), `MAX_VARS` (4), `MAX_ATOMS_PER_VAR` (16) and `MAX_GRID` (65,536 cells per state). Larger machines get `Verdict::TooLarge`.

## Limitations and scope

- **Guards are not used for reachability.** A state reported reachable might not be reachable with the real variable values. The deadlock witness is a valid valuation, but that valuation might not be reachable.
- **Compile-time checking covers only a subset.** `const_check` takes machines whose guards are conjunctions of `x <op> c` atoms with integer `c`, written as plain data. Other guards, including rational thresholds and multi-variable linear guards, need the run-time `analyze`. Missing transitions in the `Spec` are not detected, so the spec must match the real machine.
- **The const check does not read your machine.** It checks the `Spec` you write. Nothing links a `Spec` to a `Machine` or to your real state machine code.
- **Single initial state.** Analysis starts from one state.
- **Overflow is reported, not hidden.** If the solver cannot decide a guard, `analyze` returns `Error::Unknown`.

## Related crates

- Depends on [`tpt-dev-solver`](https://docs.rs/tpt-dev-solver), and re-exports `Arena`, `Formula`, `Linear`, `Rational` and `Var`.
- No workspace crate depends on it.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

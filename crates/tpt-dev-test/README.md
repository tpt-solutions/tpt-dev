# tpt-dev-test

Property-based testing with invariant generation for arithmetic constraints.

## Overview

`tpt-dev-test` is a small property-testing library for arithmetic code. It generates inputs, runs a property over them, shrinks a failing input to a small counterexample, and lets you replay any case from a seed. It adds two things that plain random testing lacks: generators whose values are guaranteed to satisfy a linear constraint, and a search for inductive invariants of simple transition systems. Both use [`tpt-dev-solver`](https://docs.rs/tpt-dev-solver).

It is for people who test integer or rational arithmetic and want reproducible runs without pulling in a large framework. The runner and generators are `no_std` with `alloc`. [`tpt-dev-fuzz`](https://docs.rs/tpt-dev-fuzz) builds on the constrained generator to produce fuzz seeds.

## Features

- **Deterministic runs:** each case has its own seed, derived from the run seed and the case index. A failure reports the seed and case, and `replay` regenerates that input.
- **Generators and shrinking:** `IntRange`, `Bools`, `VecOf` and `Pair`, plus the `Generator` trait for your own. Shrinking is greedy and bounded by `max_shrinks`.
- **Solver-constrained generation:** `ConstrainedGen` produces values that satisfy a linear real constraint. Random lower bounds spread the samples out.
- **Invariant inference:** `infer` proposes linear bounds from samples. `prove` checks each one with the solver, and returns either `Holds` or a counterexample that the solver's model confirms.
- **Inductive invariants for transition systems:** `find_invariant` samples random walks, mines bounds that hold on every sampled state, and drops candidates with Houdini-style filtering. The solver checks initiation and consecution. It returns `Proven` only when every check is `Unsat`, `Violated` with a concrete trace when a walk breaks the property, `Vacuous` when no state is initial, and `NoInvariant` otherwise.
- **Own RNG:** `Rng` and `Seed` are a small seeded generator, so results do not depend on an external crate.

## Installation

```sh
cargo add tpt-dev-test
```

The crate is `#![no_std]` and needs `alloc`. It depends only on `tpt-dev-solver`.

| Feature | Enables | Default |
| ------- | ------- | ------- |
| `std`   | Links `std`. The library has no `std`-only code today, so the flag changes nothing in the public API. | off |

## Quick start

Run a property, and get a shrunk counterexample when it fails:

```rust
use tpt_dev_test::{Config, IntRange, check, replay};

let config = Config::default();
let generator = IntRange::new(0, 1000);
let failure = check(&config, &generator, |&x| x < 100).unwrap_err();
assert_eq!(failure.minimal, 100);
// Regenerate exactly the input that first failed.
assert_eq!(replay(&config, failure.case, &generator), failure.original);
```

Generate inputs that satisfy a linear constraint:

```rust
use tpt_dev_solver::{Linear, Rational};
use tpt_dev_test::{Config, ConstrainedGen, check};

// x + y = 10, x >= 0, y >= 0.
let generator = ConstrainedGen::new(&["x", "y"], |arena, v| {
    let sum = Linear::var(v[0]).checked_add(&Linear::var(v[1]))?;
    let ten = Linear::constant(Rational::from_int(10));
    let zero = Linear::constant(Rational::ZERO);
    let eq = arena.eq(&sum, &ten)?;
    let x_ok = arena.ge(&Linear::var(v[0]), &zero)?;
    let y_ok = arena.ge(&Linear::var(v[1]), &zero)?;
    Some(arena.and(vec![eq, x_ok, y_ok]))
})
.unwrap();

let result = check(&Config::default(), &generator, |s: &Vec<Rational>| {
    s[0].checked_add(s[1]) == Some(Rational::from_int(10))
});
assert!(result.is_ok());
```

Search for an inductive invariant of a transition system:

```rust
use tpt_dev_test::{
    Affine, Atom, Outcome, SearchConfig, Transition, TransitionSystem, find_invariant,
};

// x starts at 0 and counts up while x <= 9. Prove x <= 10.
let system = TransitionSystem::new(
    1,
    vec![Atom::eq(&[(0, 1)], 0)],
    vec![Transition::new(
        vec![Atom::le(&[(0, 1)], 9)],
        vec![Affine::new(&[(0, 1)], 1)],
    )],
    vec![Atom::le(&[(0, 1)], 10)],
)
.unwrap();
assert!(matches!(
    find_invariant(&system, &SearchConfig::default()),
    Outcome::Proven { .. }
));
```

## Design notes and guarantees

- **Reproducible.** A case's input depends only on the run seed, the case index and the generator. `replay` needs nothing else.
- **Small cases first.** The size passed to generators grows with the case index, up to `Config::max_size`.
- **Solver-checked proofs.** `find_invariant` returns `Proven` only when every solver check it made was `Unsat`. A solver `Unknown` drops a candidate or ends the search without a proof.
- **Sound over the integers.** Integer systems are checked over the reals. An invariant that is inductive over the reals holds on every integer run.
- **Counterexamples are real.** A `Violated` trace comes from an actual walk of the transitions, and a `Refuted` witness is a model of the precondition that breaks the candidate.

## Limitations and scope

- **Invariants are candidates, not a full invariant generator.** `infer` only proposes bounds on single variables and pairs, and the bounds come from the largest value seen in the samples. Samples show nothing by themselves, so every candidate needs `prove`.
- **`prove` checks one invariant against one precondition.** Inductiveness of transition systems is handled by `find_invariant`, which only searches conjunctions of linear bounds from its candidate pool. It cannot find disjunctive or non-linear invariants, and it does not use counterexamples to strengthen candidates.
- **Integer systems are checked over the reals.** A proof holds for integer runs too, but integer-only invariants (such as parity) are not found.
- **Shrinking is greedy.** It can stop at a local minimum when the failing set is not monotone.
- **Constrained sampling is not uniform.** Models are drawn from the solver with random lower bounds, so the distribution is biased toward the solver's choices.
- **Real-valued samples.** `ConstrainedGen` yields `Vec<Rational>`. Integer-only inputs need filtering, as `tpt-dev-fuzz` does.

## Related crates

- Depends on [`tpt-dev-solver`](https://docs.rs/tpt-dev-solver).
- Used by [`tpt-dev-fuzz`](https://docs.rs/tpt-dev-fuzz), which draws fuzz seeds from `ConstrainedGen`.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

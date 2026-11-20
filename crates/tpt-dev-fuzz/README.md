# tpt-dev-fuzz

Solver-guided structural fuzzer that generates inputs satisfying linear preconditions.

## Overview

Plain random fuzzing rarely satisfies a linear equality over several inputs, so bugs that sit behind one stay hidden. `tpt-dev-fuzz` asks [`tpt-dev-solver`](https://docs.rs/tpt-dev-solver) for integer inputs that meet a linear precondition, uses them as the starting corpus, and then mutates them with coverage feedback.

It is a small, in-process fuzzer for functions over a fixed number of `i64` inputs. Targets are closures that report coverage edges and return `Pass` or `Crash`. Seeds come from [`tpt-dev-test`](https://docs.rs/tpt-dev-test)'s constrained generator. The crate is `no_std` with `alloc`, so it is not tied to a host fuzzing engine.

## Features

- **Structured inputs:** a fixed number of integer inputs, each clamped to configurable bounds during mutation.
- **Solver seeds:** `solver_seeds` takes a linear precondition and returns integer inputs that satisfy it. Models are over the reals, so only integral ones are kept.
- **Coverage feedback:** targets report edges with `Coverage::hit`. An input that reaches a new edge joins the corpus, and later mutations start from corpus entries.
- **Crash minimisation:** `minimise` shrinks a crashing input toward zero while it still crashes. `Fuzzer::run` applies it to the crash it returns.
- **Deterministic:** a seed fixes every mutation choice, and `solver_seeds` uses a fixed internal seed.

## Installation

```sh
cargo add tpt-dev-fuzz
```

The crate is `#![no_std]` and needs `alloc`. It depends on `tpt-dev-solver` and `tpt-dev-test`.

| Feature | Enables | Default |
| ------- | ------- | ------- |
| `std`   | Links `std`. The library has no `std`-only code today, so the flag changes nothing in the public API. | off |

## Quick start

```rust
use tpt_dev_fuzz::{Config, Coverage, Fuzzer, Outcome, solver_seeds};
use tpt_dev_solver::{Linear, Rational};

// The bug needs a + b = 7 exactly, which random mutation rarely hits.
let target = |v: &[i64], cov: &mut Coverage| {
    cov.hit(0);
    if v[0] + v[1] == 7 && v[0] > 3 {
        cov.hit(1);
        return Outcome::Crash(String::from("a + b = 7 with a > 3"));
    }
    Outcome::Pass
};

// Ask the solver for inputs that satisfy the precondition a + b = 7, a > 3.
let seeds = solver_seeds(2, 8, |arena, v| {
    let sum = Linear::var(v[0]).checked_add(&Linear::var(v[1]))?;
    let eq = arena.eq(&sum, &Linear::constant(Rational::from_int(7)))?;
    let big = arena.gt(&Linear::var(v[0]), &Linear::constant(Rational::from_int(3)))?;
    Some(arena.and(vec![eq, big]))
})
.unwrap();

let mut fuzzer = Fuzzer::new(target, 2, Config::default());
let found = fuzzer.run(&seeds).expect("a seed already hits the bug");
assert_eq!(found.input[0] + found.input[1], 7);
```

Without seeds, `fuzzer.run(&[])` starts from the all-zero input and relies on mutation alone.

## Design notes and guarantees

- **Deterministic.** The mutation choices come from `Config::seed`. The same configuration, target and seeds give the same run.
- **Bounded.** A run stops after `Config::iterations` mutated inputs. The corpus holds at most `Config::corpus_cap` entries, and a random entry is replaced when it is full.
- **Seeds do not count as iterations.** `Found::iterations` is 0 when a seed already crashes the target.
- **Solver seeds satisfy the precondition.** Each returned seed is an integral model of it. `solver_seeds` makes a bounded number of attempts and returns `NoIntegerSolutions` if none is integral.
- **Demonstration.** `tests/guided.rs` has a bug behind `a + b + c + d = 1234`. Plain fuzzing does not find it within 20,000 iterations. Solver seeds satisfy the equation directly, so the bug is found on the first run.

## Limitations and scope

- **Integer inputs only.** The solver works over the reals, so seeds are rejected when the model is fractional. A precondition whose only solutions are fractional gives `NoIntegerSolutions`.
- **Linear preconditions.** Anything the solver's subset cannot express, such as nonlinear terms, cannot seed the fuzzer.
- **Only the seeds are solver-derived.** Mutations are random and do not respect the precondition after the first step.
- **Corpus is bounded but simple.** When it is full, a random entry is replaced. There is no minimisation of the corpus itself.
- **No external process or sanitizer hooks.** Targets are in-process closures, so a target that aborts ends the run.
- **Manual coverage.** The target must call `Coverage::hit` itself. There is no instrumentation.

## Related crates

- Depends on [`tpt-dev-solver`](https://docs.rs/tpt-dev-solver) for the constraint types and on [`tpt-dev-test`](https://docs.rs/tpt-dev-test) for `ConstrainedGen` and the seeded RNG.
- No workspace crate depends on it.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

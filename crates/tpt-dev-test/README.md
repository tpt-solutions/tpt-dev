# tpt-dev-test

Property-based testing for arithmetic. Inputs are generated, failures are shrunk to small counterexamples, and runs are reproducible from a seed. Constrained generation and invariant inference use `tpt-dev-solver`.

- **Deterministic runs:** each case has its own seed, derived from the run seed and the case index. A failure reports the seed and case, and `replay` regenerates that input.
- **Generators and shrinking:** `IntRange`, `Bools`, `VecOf` and `Pair`. Shrinking is greedy and bounded by `max_shrinks`.
- **Solver-constrained generation:** `ConstrainedGen` produces values that satisfy a linear real constraint. The random bounds spread the samples out.
- **Invariant inference:** `infer` proposes linear bounds from samples. `prove` checks each one with the solver, and returns either `Holds` or a counterexample that the solver's model confirms.
- **Inductive invariants for transition systems:** `find_invariant` samples random walks, mines bounds that hold on every sampled state, and drops candidates with Houdini-style filtering. The solver checks initiation and consecution. It returns `Proven` only when every check is `Unsat`, `Violated` with a concrete trace when a walk breaks the property, and `NoInvariant` otherwise.
- **`no_std` with `alloc`:** the runner and generators need no standard library.

## Example

```rust
use tpt_dev_test::{Config, IntRange, check};

let failure = check(&Config::default(), &IntRange::new(0, 1000), |&x| x < 100)
    .unwrap_err();
assert_eq!(failure.minimal, 100);
// Rerun exactly this case with `replay(&config, failure.case, &gen)`.
```

## Limits

- **Invariants are candidates, not a full invariant generator.** `infer` only proposes bounds on single variables and pairs, and the bounds come from the largest value seen in the samples. Samples show nothing by themselves, so every candidate needs `prove`.
- **`prove` checks one invariant against one precondition.** Inductiveness of transition systems is handled by `find_invariant`, which only searches conjunctions of linear bounds from its candidate pool. It cannot find disjunctive or non-linear invariants, and it does not use counterexamples to strengthen candidates.
- **Integer systems are checked over the reals.** A proof holds for integer runs too, but integer-only invariants (such as parity) are not found.
- **Shrinking is greedy.** It can stop at a local minimum when the failing set is not monotone.
- **Constrained sampling is not uniform.** Models are drawn from the solver with random lower bounds, so the distribution is biased toward the solver's choices.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

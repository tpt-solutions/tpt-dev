# tpt-dev-fuzz

Solver-guided fuzzer. Plain random fuzzing rarely satisfies a linear equality over several inputs. This fuzzer asks `tpt-dev-solver` for inputs that do, and then mutates them with coverage feedback.

- **Structured inputs:** a fixed number of integer inputs, each with its own bounds.
- **Solver seeds:** `solver_seeds` takes a linear precondition and returns integer inputs that satisfy it. Models are over the reals, so only integral ones are kept.
- **Coverage feedback:** targets report edges with `Coverage::hit`. An input that reaches a new edge joins the corpus, and later mutations start from corpus entries.
- **Crash minimisation:** `minimise` shrinks a crashing input toward zero while it still crashes.
- **Deterministic:** a seed fixes every mutation choice.

## Example

```rust
use tpt_dev_fuzz::{Config, Coverage, Fuzzer, Outcome, solver_seeds};

// Crashes when the first input is 42.
let target = |v: &[i64], cov: &mut Coverage| {
    cov.hit(0);
    if v[0] == 42 {
        cov.hit(1);
        return Outcome::Crash(String::from("hit 42"));
    }
    Outcome::Pass
};
let mut fuzzer = Fuzzer::new(target, 1, Config { iterations: 50_000, ..Config::default() });
let found = fuzzer.run(&[]).unwrap();
assert_eq!(found.input, vec![42]);
```

The demo in `tests/guided.rs` uses a bug behind `a + b + c + d = 1234`. Plain fuzzing does not find it within 20,000 iterations. Solver seeds satisfy the equation directly, so the bug is found on the first run.

## Limits

- **Integer inputs only.** The solver works over the reals, so seeds are rejected when the model is fractional. A precondition whose only solutions are fractional gives `NoIntegerSolutions`.
- **Linear preconditions.** Anything the solver's subset cannot express, such as nonlinear terms, cannot seed the fuzzer.
- **Only the seeds are solver-derived.** Mutations are random and do not respect the precondition after the first step.
- **Corpus is bounded but simple.** When it is full, a random entry is replaced. There is no minimisation of the corpus itself.
- **No external process or sanitizer hooks.** Targets are in-process closures, so a target that aborts ends the run.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

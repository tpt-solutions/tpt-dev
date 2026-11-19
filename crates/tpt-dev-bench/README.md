# tpt-dev-bench

Benchmark statistics and a CI gate that fails a build when performance regresses.

- **Summaries:** mean, standard deviation, median, min and max.
- **Outlier trimming:** Tukey's rule (1.5 interquartile ranges) removes extreme samples before the gate looks at them.
- **Welch's t-test:** a change counts as real only when its t statistic is large. Noise is reported as inconclusive, not as a regression.
- **Baselines:** a plain text file, one line per benchmark, that is easy to review in a diff.
- **Gate policy:** a regression fails only when the slowdown is above a percentage threshold and the t statistic is trusted. Speedups are reported and never fail the gate.
- **`no_std` core:** the statistics, baseline and gate use `core` and `alloc`. `measure_with` times closures with any `CycleSource`, such as a Cortex-M DWT counter. The `std` feature adds a timing harness built on `Instant`.

## Example

```rust
use tpt_dev_bench::{Entry, Policy, judge};

let baseline = Entry { name: "parse".into(), mean: 100.0, stddev: 1.0, n: 20 };
let new_run = [130.0, 131.0, 129.0, 130.5, 129.5, 130.2];
let verdict = judge(&new_run, &baseline, Policy::default()).unwrap();
if verdict.is_failure() {
    // fail the CI job
}
```

## Bare metal

```rust
use tpt_dev_bench::{CycleSource, measure_with};

struct Dwt;
impl CycleSource for Dwt {
    fn now(&mut self) -> u64 {
        // Read DWT CYCCNT here, widened to u64 by the board code.
        0
    }
}

let mut clock = Dwt;
let cycles = measure_with(&mut clock, 10, 100, || { /* work */ });
```

## CI gate command

The `gate` example compares a results file with a baseline file and exits 1 on a trusted regression, 2 on usage or parse errors, and 0 otherwise:

```text
cargo run -p tpt-dev-bench --example gate -- baseline.txt results.txt
```

See `examples/ci/bench-gate.yml` for a pull request workflow. It has not been run in CI.

## Limits

- **Approximate critical value.** The gate uses a t threshold of about 2, which fits large samples. The exact value depends on the degrees of freedom, which are not computed.
- **Only the mean is judged.** The gate does not compare distributions or tails.
- **No automatic baseline update.** A baseline is updated by hand, with the file reviewed in version control.
- **Counter supplied by the board.** `measure_with` returns raw counter ticks. The crate does not know the clock frequency, so converting cycles to time is left to the caller.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

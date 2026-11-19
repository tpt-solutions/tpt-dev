# tpt-dev-bench

Benchmark statistics and a CI gate that fails a build when performance regresses.

- **Summaries:** mean, standard deviation, median, min and max.
- **Outlier trimming:** Tukey's rule (1.5 interquartile ranges) removes extreme samples before the gate looks at them.
- **Welch's t-test:** a change counts as real only when its t statistic is large. Noise is reported as inconclusive, not as a regression.
- **Baselines:** a plain text file, one line per benchmark, that is easy to review in a diff.
- **Gate policy:** a regression fails only when the slowdown is above a percentage threshold and the t statistic is trusted. Speedups are reported and never fail the gate.
- **`no_std` core:** the statistics, baseline and gate use `core` and `alloc`. The `std` feature adds a timing harness built on `Instant`.

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

## Limits

- **Approximate critical value.** The gate uses a t threshold of about 2, which fits large samples. The exact value depends on the degrees of freedom, which are not computed.
- **Only the mean is judged.** The gate does not compare distributions or tails.
- **No cycle-counter mode yet.** The harness measures wall-clock time with `Instant`. A `no_std` mode that reads a hardware cycle counter is not written.
- **No automatic baseline update.** A baseline is updated by hand, with the file reviewed in version control.
- **Host harness only.** The `std` harness does not run on bare metal, so bare-metal timing needs a separate measurement path.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

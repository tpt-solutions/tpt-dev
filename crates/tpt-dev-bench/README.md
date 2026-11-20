# tpt-dev-bench

Statistical benchmarking and CI gating that blocks performance regressions.

## Overview

`tpt-dev-bench` turns timing samples into a pass or fail decision. It summarises samples, trims outliers, compares a new run with a stored baseline using Welch's t-test, and fails only when a slowdown is both large and statistically trusted. Noise is reported as inconclusive, not as a regression.

The core is `#![no_std]` and needs `alloc`, so it works on embedded targets, where you supply a cycle counter. The `std` feature adds a timing harness built on `Instant`. A `gate` example turns the library into a CI command. It has no dependencies on other `tpt-dev-*` crates.

## Features

- **Summaries:** mean, standard deviation, median, min and max.
- **Outlier trimming:** Tukey's rule (1.5 interquartile ranges) removes extreme samples before the gate looks at them. Inputs under four samples are returned unchanged.
- **Welch's t-test:** a change counts as real only when its t statistic is large. Noise is reported as inconclusive.
- **Baselines:** a plain text file, one `name mean stddev n` line per benchmark, easy to review in a diff.
- **Gate policy:** a regression fails only when the slowdown is above a percentage threshold and the t statistic is trusted. Speedups are reported and never fail the gate.
- **Bare-metal timing:** `measure_with` times closures with any `CycleSource`, such as a Cortex-M DWT counter.
- **Host timing:** with `std`, `harness::measure` times closures in nanoseconds.

## Installation

```sh
cargo add tpt-dev-bench
```

| Feature | Enables | Default |
| --- | --- | --- |
| `std` | The `harness` module (`harness::measure`, using `std::time::Instant`). Also required by the `bench_suite` example. | off |

The crate is `#![no_std]` and uses `alloc`. CI builds it for `thumbv7em-none-eabihf`.

## Quick start

```rust
use tpt_dev_bench::{Entry, Policy, Verdict, judge};

let baseline = Entry { name: "parse".into(), mean: 100.0, stddev: 1.0, n: 20 };
let new_run = [130.0, 131.0, 129.0, 130.5, 129.5, 130.2];

let verdict = judge(&new_run, &baseline, Policy::default()).unwrap();
assert!(verdict.is_failure());
assert!(matches!(verdict, Verdict::Regression { .. }));
```

`Policy::default()` fails on a slowdown of more than 10 percent whose t statistic reaches the two-sided 95% critical value for the comparison's Welch degrees of freedom. Set `Policy::critical_t` to `Some(value)` to use a fixed critical value instead.

### Bare metal

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
assert_eq!(cycles.len(), 100);
```

`measure_with` returns one sample per timed run, in counter ticks. It uses wrapping subtraction, so a counter that wraps between two reads still gives the right difference.

## More examples

- **`gate`:** compares a results file with a baseline file. Both use the baseline text format. It exits 0 with no trusted regression (improvements and inconclusive changes pass), 1 on a trusted regression, and 2 on a usage error, unreadable file, parse error, or a benchmark it cannot judge.

  ```text
  cargo run -p tpt-dev-bench --example gate -- baseline.txt results.txt [--max-regression-pct PCT] [--critical-t T]
  ```

- **`bench_suite`:** a template benchmark runner. It measures three fixed workloads of this crate and writes a results file in the baseline format. Copy it and replace the workloads to benchmark your own code.

  ```sh
  cargo run --release -p tpt-dev-bench --example bench_suite --features std -- results.txt
  ```

- **`examples/ci/bench-gate.yml`:** a pull request workflow template that runs `bench_suite` and then `gate`. It has not been run in CI. The `examples/ci/baseline.txt` it points to holds numbers from one machine, so regenerate it for your runners.

## Design notes and guarantees

- **No `std` math.** Square root is computed by Newton's method, so the statistics work in `no_std`.
- **Conservative critical value.** `t_critical_95` uses a table and rounds the degrees of freedom down, so its result is never smaller than the exact value.
- **Degenerate inputs return `None`,** not a verdict: fewer than two samples, or a zero baseline mean.
- **Zero variance is handled.** If both standard errors are zero, an unchanged mean gives t = 0 and a changed mean gives an infinite t.
- **Speedups never fail the gate.**
- **Baselines are deterministic text,** written by `Baseline::to_text` and read back by `Baseline::parse`, which reports the line and reason for a bad line.

## Limitations and scope

- **Only the mean is judged.** The gate does not compare distributions or tails.
- **No automatic baseline update.** A baseline is updated by hand, with the file reviewed in version control.
- **Counter supplied by the board.** `measure_with` returns raw counter ticks. The crate does not know the clock frequency, so converting cycles to time is left to the caller.
- **`judge_entry` trusts its input.** A results entry must already have had its outliers trimmed.
- **Noisy runners.** Shared CI machines give noisy timings. Expect occasional inconclusive results.

## Related crates

`tpt-dev-bench` does not depend on, and is not depended on by, any other workspace crate. [`tpt-dev-profiler`](https://docs.rs/tpt-dev-profiler) shows where time goes, where this crate answers whether it changed.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

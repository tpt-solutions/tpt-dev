# tpt-dev-profiler

Sampling profiler with function-level attribution and bounded per-sample cost.

## Overview

`tpt-dev-profiler` records stack samples from a timer or interrupt hook, attributes each program counter to a function using a symbol table you supply, and reports the result as a text table or as folded stacks for a flamegraph.

It is for embedded firmware and host code where you want to see where time goes without a heavy tool. The sample buffer is a fixed array of atomics, so recording is lock-free and does not allocate, and can run in an interrupt. The crate is `#![no_std]` and uses `alloc` for reports. It has no dependencies on other `tpt-dev-*` crates.

## Features

- **Sampler:** `Sampler<N, D>` holds `N` samples of up to `D` frames. A full buffer drops new samples and counts them.
- **Symbols:** `Symbols` maps a program counter to the function that contains it, by binary search over a sorted table.
- **Report:** `Report` gives self and inclusive counts per function. `write_text` writes a table of self and total percentages. `write_folded` writes stacks in the format read by `inferno-flamegraph` and `flamegraph.pl`.
- **Overhead measurement:** `examples/overhead.rs` measures the cost of one sample on the host.

## Installation

```sh
cargo add tpt-dev-profiler
```

| Feature | Enables | Default |
| --- | --- | --- |
| `std` | Links `std`. It adds no public items today. | off |

The crate is `#![no_std]`, needs `alloc` (for `Report`), and is built for `thumbv7em-none-eabihf` in CI.

## Quick start

```rust
use tpt_dev_profiler::{Report, Sampler, Symbols};

// Functions in address order, and the end of the code. Generate this from the binary.
static TABLE: &[(usize, &str)] = &[(0x100, "main"), (0x200, "sensor_read"), (0x300, "filter")];

let sampler: Sampler<64, 4> = Sampler::new();

// Called from the timer interrupt. `frames` is innermost first.
sampler.record(&[0x210, 0x110]); // sensor_read <- main
sampler.record(&[0x310, 0x210, 0x110]); // filter <- sensor_read <- main
sampler.record(&[0x120]); // main

let symbols = Symbols::new(TABLE, 0x400).unwrap();
let report = Report::new(&sampler, &symbols);
assert_eq!(report.total(), 3);

let mut folded = String::new();
report.write_folded(&mut folded).unwrap();
assert_eq!(folded, "main 1\nmain;sensor_read 1\nmain;sensor_read;filter 1\n");
```

`Sampler::new` is `const`, so a sampler can be a `static`. `record` returns `false` when the sample was dropped.

## More examples

- **`overhead`:** measures what one `record` call costs on this host, best of 64 rounds.

  ```sh
  cargo run --release -p tpt-dev-profiler --example overhead
  ```

  The figures are for the host only. The cost on a target depends on its clock, memory and interrupt entry code.

## Design notes and guarantees

- **No allocation and no locks in `record`.** Every slot is written with atomic stores, and a sample is published with a release store.
- **Bounded cost.** A call copies at most `D` frames, so its time is bounded by `D`. Memory is fixed at `N` samples of `D` frames.
- **Preemption is tolerated.** If a higher-priority hook preempts another, the two calls take different slots. The reader skips a slot until its write is published, and stops at the first unpublished slot.
- **Samples are never overwritten.** A full buffer drops new samples, and `dropped()` counts them.
- **Concurrent recording is tested** in `tests/concurrent.rs`.

## Limitations and scope

- **The hook is yours to install.** The crate records samples. It does not configure a timer or install an interrupt handler, because those are specific to each board.
- **Stacks come from the caller.** There is no unwinder. The hook must supply the frames, for example by following frame pointers. Without them, each sample is only the innermost program counter.
- **Symbols come from the caller.** The table must be sorted by address, and `code_end` is where the code ends. A program counter before the first function, or at or past `code_end`, is reported as `[unknown]`.
- **Overhead is bounded, not zero.** Each sample costs time that grows with the stack depth `D`. Measure it on the target before you rely on a sampling rate.
- **Bounded memory.** A full buffer drops samples. Clear it with `clear`, or size it for the run.
- **No rendered flamegraph.** The crate writes folded stacks. Render them with `inferno-flamegraph` or `flamegraph.pl`.

## Related crates

`tpt-dev-profiler` does not depend on, and is not depended on by, any other workspace crate. [`tpt-dev-bench`](https://docs.rs/tpt-dev-bench) measures and gates run time, where this crate shows where it is spent.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

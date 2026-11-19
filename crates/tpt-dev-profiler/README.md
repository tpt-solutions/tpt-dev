# tpt-dev-profiler

A sampling profiler for embedded and host code. A timer or interrupt hook records the program counters of the running code. The crate attributes each sample to a function, using a symbol table that you supply, and reports the result as text or as folded stacks for a flamegraph.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

- **Sampler:** `Sampler<N, D>` holds `N` samples of up to `D` frames in a fixed array of atomics. It is lock-free and does not allocate, so the hook can run in an interrupt. A full buffer drops new samples and counts them.
- **Symbols:** `Symbols` maps a program counter to the function that contains it, by binary search over a sorted table.
- **Report:** `Report` gives self and inclusive counts per function. `write_text` writes a table. `write_folded` writes stacks in the format read by `inferno-flamegraph` and `flamegraph.pl`.
- **Overhead:** `examples/overhead.rs` measures the cost of one sample on the host.

## Example

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

## Limits

- **The hook is yours to install.** The crate records samples. It does not configure a timer or install an interrupt handler, because those are specific to each board.
- **Stacks come from the caller.** There is no unwinder. The hook must supply the frames, for example by following frame pointers. Without them, each sample is only the innermost program counter.
- **Symbols come from the caller.** The table must be sorted by address, and `code_end` is where the code ends. A program counter before the first function, or at or past `code_end`, is reported as `[unknown]`.
- **Overhead is bounded, not zero.** Each sample costs time that grows with the stack depth `D`. The overhead example measures this on the host. Measure it on the target before you rely on a sampling rate.
- **Bounded memory.** A full buffer drops samples. Clear it, or size it for the run.
- **No rendered flamegraph.** The crate writes folded stacks. Render them with `inferno-flamegraph` or `flamegraph.pl`.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

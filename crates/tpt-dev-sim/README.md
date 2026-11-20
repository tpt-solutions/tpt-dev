# tpt-dev-sim

Cycle-accurate hardware simulator in safe Rust.

## Overview

`tpt-dev-sim` simulates synchronous hardware one clock cycle at a time. Components are clocked: each one ticks once per cycle, reads the signal values committed in the previous cycle, and writes the values that take effect at the end of this cycle. That is how synchronous hardware behaves, so results do not depend on the order in which components are listed.

It is for testing firmware-adjacent logic, such as a peripheral model or a timing rule, on the host, with deterministic results and waveform output. The kernel holds a fixed number of signals (up to 64 bits wide each) and allocates nothing, so it also builds for `no_std` targets. The example peripheral, `timer::Timer`, is driven through [`tpt-dev-mem`](https://docs.rs/tpt-dev-mem) typed registers in the crate's integration test, the same way firmware would drive a real one.

## Features

- **Kernel:** `Kernel<N>` holds up to `N` signals of 1 to 64 bits and steps them one cycle at a time. It allocates nothing.
- **Components:** implement `Component::tick`. A component reads and writes signals through a `Ctx`.
- **Failures:** a component can call `ctx.fail(..)`. Two components driving one signal in the same cycle also count as a failure. The kernel keeps the count and the first failure.
- **Timing checks:** `Deadline` (a response must follow a trigger within a window) and `MinGap` (pulses must not be too close together).
- **Waveforms:** `Vcd` writes Value Change Dump output to any `core::fmt::Write`, readable by viewers such as GTKWave.
- **Peripheral example:** `timer::Timer`, a periodic timer with `CTRL`, `PERIOD` and `COUNT` registers and an interrupt pulse.

## Installation

```sh
cargo add tpt-dev-sim
```

The crate is `#![no_std]`, needs no allocator and has no runtime dependencies. It is not published yet; until then, depend on it by path or git. `tpt-dev-mem` is a dev-dependency, used only by the timer integration test.

| Feature | Enables | Default |
|---|---|---|
| `std` | links `std` (used for tests and host tooling) | off |

The CI builds the workspace for `thumbv7em-none-eabihf`.

## Quick start

```rust
use tpt_dev_sim::{Component, Ctx, Kernel, SignalId};

struct Counter {
    value: SignalId,
}

impl Component for Counter {
    fn tick(&mut self, ctx: &mut Ctx<'_>) {
        let next = ctx.read(self.value) + 1;
        ctx.write(self.value, next);
    }
}

let mut sim: Kernel<4> = Kernel::new();
let count = sim.signal("count", 8, 0).unwrap();
let mut counter = Counter { value: count };
for _ in 0..300 {
    sim.step(&mut [&mut counter]);
}
assert_eq!(sim.value(count), 300 % 256);
```

The 8-bit counter above wraps at 256 because values are kept to the signal's width.

A waveform and a timing check, using the periodic timer:

```rust
use core::cell::Cell;
use tpt_dev_sim::timer::{CTRL, CTRL_ENABLE, CTRL_IRQ_ENABLE, PERIOD};
use tpt_dev_sim::{Kernel, MinGap, Timer, Vcd};

let regs = [Cell::new(0u32), Cell::new(0), Cell::new(0)];
let mut sim: Kernel<4> = Kernel::new();
let count = sim.signal("count", 32, 0).unwrap();
let irq = sim.signal("irq", 1, 0).unwrap();
let mut timer = Timer::new(&regs, count, irq);
let mut gap = MinGap::new(irq, 4);

regs[PERIOD].set(4);
regs[CTRL].set(CTRL_ENABLE | CTRL_IRQ_ENABLE);

let mut vcd = Vcd::new(String::new(), &sim, "timer").unwrap();
for _ in 0..40 {
    sim.step(&mut [&mut timer, &mut gap]);
    vcd.sample(&sim).unwrap();
}
assert_eq!(sim.failures(), 0);
assert!(vcd.into_inner().contains("$var"));
```

## More examples

The `tests/timer_mmio.rs` integration test shows firmware programming the timer through typed `tpt-dev-mem` `Reg` handles over the same register memory the model uses. Run it with `cargo test -p tpt-dev-sim --test timer_mmio`. The crate has no `examples/` directory.

## Design notes and guarantees

- **Two-phase cycles.** `Kernel::step` copies committed values to the "next" buffer, ticks every component, then commits. A component cannot see another's writes in the same cycle, so the component order does not affect the result.
- **Deterministic conflicts.** If two components write the same signal in one cycle, a failure is recorded and the later write wins, so the outcome is still repeatable.
- **Fixed storage.** Signal names, widths and values are fixed arrays sized by `N`. `Kernel::signal` returns `SimError::Full` when the kernel has no free slot and `SimError::Width` for a width of zero or more than 64.
- **Failures are data.** `failures()` is a count, and `first_failure()` returns the cycle, the component index and a static message.
- **Timer behaviour.** Registers are at indices 0 (`CTRL`), 1 (`PERIOD`) and 2 (`COUNT`, read-only). While enabled the counter rises by one each cycle, wraps to zero and pulses `irq` for one cycle when it reaches `PERIOD`. A period of zero stops counting. Disabling holds the counter.
- **Deadline semantics.** `Deadline` checks that `response` is high within `within` cycles of each rising edge of `trigger`. A trigger that rises again before the response arrives does not start a new window.
- **Safe Rust.** The library code uses no `unsafe`. The `unsafe` in the integration test belongs to the `tpt-dev-mem` register handles.

## Limitations and scope

- **One clock.** Every component ticks on the same clock. Multiple clock domains and clock ratios are not modelled.
- **Every value is registered.** A component sees another component's output one cycle later. Combinational paths between components are not modelled; a wire that depends on its inputs in the same cycle must be written as part of one component.
- **No CPU model.** The timer is a peripheral. A processor core is not included, so firmware is driven by test code.
- **Failures are counted, not stopped.** The kernel records failures and keeps running. It does not halt on the first one.
- **Waveform names.** VCD names are written as given, so use plain identifiers.
- **Not checked against hardware.** Timing follows the model's rules, not a real device's.
- **Static names.** Signal names and failure messages are `&'static str`.

## Related crates

- [`tpt-dev-mem`](https://docs.rs/tpt-dev-mem): typed memory-mapped registers; a dev-dependency used to drive the simulated timer.
- [`tpt-dev-async`](https://docs.rs/tpt-dev-async): its documentation points to cycle-accurate simulators for measuring worst-case timing.

## Status and MSRV

Pre-release (0.0.0, not yet published). The minimum supported Rust version is 1.98 (`rust-version` in `Cargo.toml`).

## License

Licensed under either of MIT or Apache-2.0, at your option.

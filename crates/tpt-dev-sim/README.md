# tpt-dev-sim

A cycle-accurate hardware simulator in safe Rust. Components are clocked: each one ticks once per cycle, reads the signal values committed in the previous cycle, and writes the values that take effect at the end of this cycle. That is how synchronous hardware behaves, so results do not depend on the order components are listed in.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

- **Kernel:** a fixed number of signals, up to 64 bits wide, allocated in a `Kernel<N>`. It allocates nothing, so it builds for `no_std` targets.
- **Components:** implement `Component::tick`. A component reads and writes signals through a `Ctx`.
- **Failures:** a component can call `ctx.fail(..)`. Two components driving one signal in the same cycle also count as a failure. The kernel keeps the count and the first failure.
- **Timing checks:** `Deadline` (a response must follow a trigger within a window) and `MinGap` (pulses must not be too close).
- **Waveforms:** `Vcd` writes Value Change Dump output to any `core::fmt::Write`.
- **Peripheral example:** `timer::Timer`, a periodic timer. Firmware reaches its registers through `tpt-dev-mem` typed `Reg` handles, and the model reads and writes the same memory.

## Example

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

## Limits

- **One clock.** Every component ticks on the same clock. Multiple clock domains and clock ratios are not modelled.
- **Every value is registered.** A component sees another component's output one cycle later. Combinational paths between components are not modelled; a wire that depends on its inputs in the same cycle must be written as part of one component.
- **No CPU model.** The timer is a peripheral. A processor core is not included, so firmware is driven by test code.
- **Failures are counted, not stopped.** The kernel records failures and keeps running. It does not halt on the first one.
- **Waveform names.** VCD names are written as given, so use plain identifiers.
- **Not checked against hardware.** Timing follows the model's rules, not a real device's.

//! A periodic timer peripheral.
//!
//! The timer has three 32-bit registers, at byte offsets 0, 4 and 8. Firmware
//! reaches them through `tpt-dev-mem` typed registers, and the model reads and
//! writes the same memory:
//!
//! | Index | Register | Access | Meaning |
//! |-------|----------|--------|---------|
//! | 0 | `CTRL` | RW | bit 0 enables counting; bit 1 enables the interrupt pulse |
//! | 1 | `PERIOD` | RW | the counter wraps to zero after `PERIOD` cycles |
//! | 2 | `COUNT` | RO | the current counter value |
//!
//! While enabled, the counter goes up by one each cycle. It wraps to zero, and
//! `irq` pulses high for one cycle, when it reaches `PERIOD`. A period of zero
//! stops counting. Disabling the timer holds the counter at its value.

use core::cell::Cell;

use crate::kernel::{Component, Ctx, SignalId};

/// Register index of the control register.
pub const CTRL: usize = 0;
/// Register index of the period register.
pub const PERIOD: usize = 1;
/// Register index of the read-only count register.
pub const COUNT: usize = 2;
/// `CTRL` bit that enables counting.
pub const CTRL_ENABLE: u32 = 1;
/// `CTRL` bit that enables the interrupt pulse.
pub const CTRL_IRQ_ENABLE: u32 = 2;

/// A periodic timer. Its registers are borrowed from the caller, so firmware
/// can reach the same memory.
pub struct Timer<'a> {
    regs: &'a [Cell<u32>; 3],
    count: SignalId,
    irq: SignalId,
}

impl<'a> Timer<'a> {
    /// A timer whose registers are `regs`, driving the `count` and `irq` signals.
    ///
    /// The `count` signal must be 32 bits wide and `irq` one bit wide.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::cell::Cell;
    /// use tpt_dev_sim::Kernel;
    /// use tpt_dev_sim::timer::{CTRL_ENABLE, Timer};
    ///
    /// // Enabled, with a period of 3: the counter goes 0, 1, 2 and wraps to 0.
    /// let regs = [Cell::new(CTRL_ENABLE), Cell::new(3), Cell::new(0)];
    /// let mut sim: Kernel<2> = Kernel::new();
    /// let count = sim.signal("count", 32, 0).unwrap();
    /// let irq = sim.signal("irq", 1, 0).unwrap();
    /// let mut timer = Timer::new(&regs, count, irq);
    /// for _ in 0..3 {
    ///     sim.step(&mut [&mut timer]);
    /// }
    /// assert_eq!(sim.value(count), 0);
    /// ```
    pub const fn new(regs: &'a [Cell<u32>; 3], count: SignalId, irq: SignalId) -> Self {
        Self { regs, count, irq }
    }
}

impl Component for Timer<'_> {
    fn tick(&mut self, ctx: &mut Ctx<'_>) {
        let ctrl = self.regs[CTRL].get();
        let period = self.regs[PERIOD].get();
        let count = ctx.read(self.count) as u32;
        let running = ctrl & CTRL_ENABLE != 0 && period != 0;
        let (next, matched) = if running {
            let bumped = count.saturating_add(1);
            if bumped >= period {
                (0, true)
            } else {
                (bumped, false)
            }
        } else {
            (count, false)
        };
        ctx.write(self.count, u64::from(next));
        let pulse = matched && ctrl & CTRL_IRQ_ENABLE != 0;
        ctx.write(self.irq, u64::from(pulse));
        self.regs[COUNT].set(next);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::kernel::Kernel;
    use std::vec::Vec;

    #[test]
    fn fires_once_per_period_and_holds_when_disabled() {
        let regs = [Cell::new(0), Cell::new(4), Cell::new(0)];
        let mut sim: Kernel<2> = Kernel::new();
        let count = sim.signal("count", 32, 0).unwrap();
        let irq = sim.signal("irq", 1, 0).unwrap();
        let mut timer = Timer::new(&regs, count, irq);

        for _ in 0..8 {
            sim.step(&mut [&mut timer]);
        }
        assert_eq!(sim.value(count), 0);
        assert_eq!(sim.value(irq), 0);

        // Enabled after eight idle cycles. Counting starts at cycle 8, so the
        // counter reaches its period of 4 at cycles 12 and 16.
        regs[CTRL].set(CTRL_ENABLE | CTRL_IRQ_ENABLE);
        let mut fires = Vec::new();
        for _ in 0..8 {
            sim.step(&mut [&mut timer]);
            if sim.value(irq) == 1 {
                fires.push(sim.cycle());
            }
        }
        assert_eq!(fires, [12u64, 16]);
    }

    #[test]
    fn interrupt_pulse_needs_its_enable_bit() {
        let regs = [Cell::new(CTRL_ENABLE), Cell::new(3), Cell::new(0)];
        let mut sim: Kernel<2> = Kernel::new();
        let count = sim.signal("count", 32, 0).unwrap();
        let irq = sim.signal("irq", 1, 0).unwrap();
        let mut timer = Timer::new(&regs, count, irq);
        for _ in 0..9 {
            sim.step(&mut [&mut timer]);
            assert_eq!(sim.value(irq), 0);
        }
        assert_eq!(regs[COUNT].get(), 0);
    }
}

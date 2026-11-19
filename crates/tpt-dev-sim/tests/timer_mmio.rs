//! Firmware programs the simulated timer through typed `tpt-dev-mem` registers.
//!
//! The model and the firmware share one register file. Firmware uses volatile
//! accesses through `Reg`. The model uses the same memory through `Cell`.

#![allow(unsafe_code)]
#![allow(clippy::unwrap_used)]

use core::cell::Cell;
use std::vec::Vec;

use tpt_dev_mem::{RO, RW, Reg};
use tpt_dev_sim::timer::{COUNT, CTRL, CTRL_ENABLE, CTRL_IRQ_ENABLE, PERIOD};
use tpt_dev_sim::{Kernel, MinGap, Timer};

#[test]
fn firmware_programs_the_timer_through_typed_registers() {
    let regs = [Cell::new(0u32), Cell::new(0), Cell::new(0)];
    let base = regs.as_ptr() as usize;
    let stride = core::mem::size_of::<Cell<u32>>();
    let at = |index: usize| base + index * stride;

    let (ctrl, period, count): (Reg<u32, RW>, Reg<u32, RW>, Reg<u32, RO>) = {
        // SAFETY: each address is the start of one `Cell<u32>` in `regs`. `Cell<u32>`
        // has the layout of `u32`, and `regs` outlives every register handle.
        unsafe {
            (
                Reg::from_addr(at(CTRL)),
                Reg::from_addr(at(PERIOD)),
                Reg::from_addr(at(COUNT)),
            )
        }
    };

    let mut sim: Kernel<4> = Kernel::new();
    let count_sig = sim.signal("count", 32, 0).unwrap();
    let irq = sim.signal("irq", 1, 0).unwrap();
    let mut timer = Timer::new(&regs, count_sig, irq);
    let mut gap = MinGap::new(irq, 10);

    period.write(10);
    ctrl.write(CTRL_ENABLE | CTRL_IRQ_ENABLE);

    let mut irq_cycles = Vec::new();
    for _ in 0..25 {
        sim.step(&mut [&mut timer, &mut gap]);
        if sim.value(irq) == 1 {
            irq_cycles.push(sim.cycle());
        }
    }
    assert_eq!(irq_cycles, [10u64, 20]);
    assert_eq!(count.read(), 5);
    assert_eq!(sim.failures(), 0);

    // Firmware stops the timer. The count holds, and no more pulses arrive.
    ctrl.write(0);
    let mut later = Vec::new();
    for _ in 0..5 {
        sim.step(&mut [&mut timer, &mut gap]);
        if sim.value(irq) == 1 {
            later.push(sim.cycle());
        }
    }
    assert!(later.is_empty());
    assert_eq!(count.read(), 5);
    assert_eq!(sim.failures(), 0);
}

//! Devices: what a backend can run, what it costs, and how it runs it.

use alloc::vec::Vec;

/// The kind of processor a device is. The scheduler uses it only for reporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// General-purpose processor. Supports every operation.
    Cpu,
    /// Neural processing unit.
    Npu,
    /// Graphics processor.
    Gpu,
}

/// An operation on float vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Element-wise sum of two vectors.
    Add,
    /// Element-wise product of two vectors.
    Mul,
    /// Element-wise `max(x, 0)` of one vector.
    Relu,
    /// Sum of all elements of one vector, as a one-element vector.
    Sum,
}

impl Op {
    /// How many inputs the operation takes.
    pub const fn arity(self) -> usize {
        match self {
            Op::Add | Op::Mul => 2,
            Op::Relu | Op::Sum => 1,
        }
    }
}

/// A processor the scheduler can use.
///
/// A device says which operations it supports, estimates how long each takes,
/// and runs them. No vendor SDK is needed to implement it.
pub trait Device {
    /// The device's kind.
    fn kind(&self) -> Kind;

    /// Whether the device can run `op`.
    fn supports(&self, op: Op) -> bool;

    /// Estimated time, in ticks, to run `op` on inputs of `elements` values.
    fn cost(&self, op: Op, elements: usize) -> u64;

    /// Runs `op` on `inputs`. Returns `None` if the device does not support it.
    fn run(&mut self, op: Op, inputs: &[&[f32]]) -> Option<Vec<f32>>;
}

fn compute(op: Op, inputs: &[&[f32]]) -> Vec<f32> {
    match op {
        Op::Add => inputs[0]
            .iter()
            .zip(inputs[1])
            .map(|(a, b)| a + b)
            .collect(),
        Op::Mul => inputs[0]
            .iter()
            .zip(inputs[1])
            .map(|(a, b)| a * b)
            .collect(),
        Op::Relu => inputs[0]
            .iter()
            .map(|&x| if x > 0.0 { x } else { 0.0 })
            .collect(),
        Op::Sum => alloc::vec![inputs[0].iter().sum()],
    }
}

/// The reference device. It runs every operation, and it is the baseline for
/// checking the others.
///
/// ```
/// use tpt_dev_dispatch::{CpuDevice, Device, Op};
///
/// let mut cpu = CpuDevice::default();
/// let out = cpu.run(Op::Add, &[&[1.0, 2.0], &[10.0, 20.0]]).unwrap();
/// assert_eq!(out, [11.0, 22.0]);
/// ```
#[derive(Clone, Debug, Default)]
pub struct CpuDevice {
    /// Ticks per element. Lower is faster.
    pub per_element: u64,
}

impl Device for CpuDevice {
    fn kind(&self) -> Kind {
        Kind::Cpu
    }

    fn supports(&self, _op: Op) -> bool {
        true
    }

    fn cost(&self, _op: Op, elements: usize) -> u64 {
        // A fixed overhead plus a per-element cost. The default rate is one tick per element.
        let rate = if self.per_element == 0 {
            1
        } else {
            self.per_element
        };
        50 + rate * elements as u64
    }

    fn run(&mut self, op: Op, inputs: &[&[f32]]) -> Option<Vec<f32>> {
        if inputs.len() != op.arity() {
            return None;
        }
        Some(compute(op, inputs))
    }
}

/// A simulated accelerator. It supports only the operations it is given, runs
/// them quickly, and records each call, so tests can see where work went.
#[derive(Clone, Debug)]
pub struct MockAccelerator {
    /// Operations this accelerator supports.
    pub ops: Vec<Op>,
    /// Calls made so far, in order.
    pub calls: Vec<Op>,
}

impl MockAccelerator {
    /// An accelerator that supports `ops`.
    pub fn new(ops: &[Op]) -> Self {
        Self {
            ops: ops.to_vec(),
            calls: Vec::new(),
        }
    }
}

impl Device for MockAccelerator {
    fn kind(&self) -> Kind {
        Kind::Npu
    }

    fn supports(&self, op: Op) -> bool {
        self.ops.contains(&op)
    }

    fn cost(&self, _op: Op, elements: usize) -> u64 {
        // Faster than the CPU on large inputs, with a larger fixed start-up cost.
        200 + elements as u64 / 4
    }

    fn run(&mut self, op: Op, inputs: &[&[f32]]) -> Option<Vec<f32>> {
        if !self.supports(op) || inputs.len() != op.arity() {
            return None;
        }
        self.calls.push(op);
        Some(compute(op, inputs))
    }
}

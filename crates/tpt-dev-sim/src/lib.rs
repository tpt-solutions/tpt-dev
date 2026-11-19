#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(any(test, feature = "std"))]
extern crate std;

mod check;
mod kernel;
pub mod timer;
mod vcd;

pub use check::{Deadline, MinGap};
pub use kernel::{Component, Ctx, Failure, Kernel, SignalId, SimError};
pub use timer::Timer;
pub use vcd::Vcd;

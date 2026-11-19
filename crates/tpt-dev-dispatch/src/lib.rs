#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod device;
mod graph;
mod schedule;

pub use device::{CpuDevice, Device, Kind, MockAccelerator, Op};
pub use graph::{Graph, GraphError, TaskId};
pub use schedule::{Assignment, Schedule, ScheduleError, execute, schedule};

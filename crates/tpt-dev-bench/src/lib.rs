#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod baseline;
mod cycles;
mod gate;
mod stats;

#[cfg(any(feature = "std", test))]
pub mod harness;

pub use baseline::{Baseline, Entry, ParseError};
pub use cycles::{CycleSource, ManualCycles, measure_with};
pub use gate::{Policy, Verdict, judge, judge_entry};
pub use stats::{Comparison, Summary, compare, summarise, t_critical_95, trim_outliers};

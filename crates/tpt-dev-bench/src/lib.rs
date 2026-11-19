#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod baseline;
mod gate;
mod stats;

#[cfg(any(feature = "std", test))]
pub mod harness;

pub use baseline::{Baseline, Entry, ParseError};
pub use gate::{Policy, Verdict, judge};
pub use stats::{Comparison, Summary, compare, summarise, trim_outliers};

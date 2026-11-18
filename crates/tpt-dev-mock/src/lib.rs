#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod recorder;

pub use recorder::{ExpectBuilder, Recorder};
pub use tpt_dev_mock_macros::mock;

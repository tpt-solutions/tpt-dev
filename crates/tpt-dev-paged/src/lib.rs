#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(any(test, feature = "std"))]
extern crate std;

mod pager;

pub use pager::{Error, Pager, Stats};

#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(any(test, feature = "std"))]
extern crate std;

mod field;
mod layout;

pub use field::{Endian, Field};
pub use layout::{CodecError, Layout};

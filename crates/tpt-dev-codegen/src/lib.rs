#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod ident;
mod writer;

#[cfg(any(feature = "std", test))]
pub mod build;

pub use ident::{Error, is_ident, string_literal};
pub use writer::Writer;

#![no_std]
#![doc = include_str!("../README.md")]
// The waker vtable and the task pointers use `unsafe`. Each block has a SAFETY comment.
#![allow(unsafe_code)]

#[cfg(any(test, feature = "std"))]
extern crate std;

mod executor;
mod sync;
mod timer;

pub use executor::{Executor, Full, Stats};
pub use timer::Delay;

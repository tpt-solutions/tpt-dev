#![no_std]
#![doc = include_str!("../README.md")]
// The primitives wrap `UnsafeCell` and hand out access only under the
// synchronisation rules documented on each type. Each `unsafe` block has a
// `SAFETY` comment.
#![allow(unsafe_code)]

#[cfg(any(test, feature = "std"))]
extern crate std;

mod cell;
mod order;
mod sync;

pub mod mutex;
pub mod spsc;
pub mod triple;

pub use mutex::Mutex;
pub use order::{Above, L1, L2, L3, L4, L5, L6, L7, L8, Nil, Succ, Token};
pub use spsc::Spsc;
pub use triple::TripleBuffer;

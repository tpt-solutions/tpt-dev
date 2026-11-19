#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod circuit;
mod field;
mod r1cs;

pub use circuit::{Circuit, Ring, Wire};
pub use field::{F, MODULUS_LE};
pub use tpt_dev_zk_macros::circuit;

#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(any(test, feature = "std"))]
extern crate std;

pub mod mock;
mod traits;
pub mod trustzone;

pub use traits::{
    Attestation, DIGEST_LEN, Measurement, Report, SecureCall, SecureMemory, TeeError, World,
};

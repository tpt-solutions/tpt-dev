#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod coverage;
mod fuzzer;
mod seeds;

pub use coverage::Coverage;
pub use fuzzer::{Config, Found, Fuzzer, Outcome, minimise};
pub use seeds::{SeedError, solver_seeds};

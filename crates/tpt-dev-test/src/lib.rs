#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod generator;
mod invariant;
mod rng;
mod runner;
mod solver_gen;

pub use generator::{Bools, Generator, IntRange, Pair, VecOf};
pub use invariant::{Candidate, Verdict, infer, prove};
pub use rng::{Rng, Seed};
pub use runner::{Config, Failure, check, replay};
pub use solver_gen::{ConstrainedGen, SetupError};

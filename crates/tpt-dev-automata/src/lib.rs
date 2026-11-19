#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

pub mod const_check;
mod machine;

pub use machine::{Analysis, Deadlock, DeadlockKind, Error, Machine, StateId, Transition};
pub use tpt_dev_solver::{Arena, Formula, Linear, Rational, Var};

#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod proof;
mod rational;
mod sat;
mod simplex;
mod smt;
mod solver;
mod term;

pub use proof::{Proof, ProofStep, check_proof};
pub use rational::Rational;
pub use smt::{ScriptError, Session};
pub use solver::{Budget, Farkas, Solver, Status, check_farkas};
pub use term::{Arena, FixedArena, Formula, Linear, Var};

/// The solver could not decide the problem. An intermediate value did not
/// fit in `i128`. The solver never guesses, so it reports this instead of a
/// wrong answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unknown;

impl core::fmt::Display for Unknown {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("unknown: arithmetic overflow")
    }
}

#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod grammar;
mod matcher;

pub use grammar::{Expr, Grammar, Rule};
pub use matcher::{Expected, MAX_DEPTH, ParseError, Tree};

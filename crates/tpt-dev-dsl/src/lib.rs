#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod grammar;
mod matcher;
mod packrat;

pub use grammar::{Expr, Grammar, Rule};
pub use matcher::{Expected, MAX_DEPTH, ParseError, Tree};
pub use packrat::{Memo, MemoFull, PackratError};

/// Parses a grammar written in PEG notation at compile time. Needs the `macros` feature.
#[cfg(feature = "macros")]
pub use tpt_dev_dsl_macros::dsl;

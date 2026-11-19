#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod extract;
mod render;

pub use extract::{Example, Mode, doc_comments, extract};
pub use render::{escape, example_section, inject_before, page};

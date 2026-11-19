#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

pub mod component;
pub mod link;
pub mod module;
pub mod types;

pub use component::{Component, Section, parse as parse_component};
pub use link::{Binding, LinkError, Plan, Registry};
pub use module::{ErrorKind, Module, ParseError, parse};

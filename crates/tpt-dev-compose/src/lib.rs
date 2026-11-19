#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

pub mod component;
pub mod link;
pub mod module;
pub mod types;

pub use component::{
    Component, ComponentExport, ComponentImport, ExternDesc, ExternKind, Section, Space, TypeBound,
    parse as parse_component,
};
pub use link::{Binding, ComponentBinding, LinkError, Plan, Registry};
pub use module::{ErrorKind, Module, ParseError, parse};

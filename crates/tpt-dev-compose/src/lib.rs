#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod compare;
pub mod component;
mod interp;
pub mod link;
pub mod module;
pub mod runtime;
pub mod types;

pub use component::{
    Alias, Component, ComponentExport, ComponentImport, CoreExportKind, ExternDesc, ExternKind,
    OuterKind, Section, Space, TypeBound, parse as parse_component,
};
pub use link::{Binding, ComponentBinding, LinkError, Plan, Registry};
pub use module::{ErrorKind, Module, ParseError, parse};
pub use runtime::{Feature, Runtime, RuntimeError, Trap, Value};

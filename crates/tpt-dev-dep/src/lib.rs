//! Dependency graph analysis: cycles, license policy and transitive size.
//!
//! The graph comes from a `Cargo.lock` file. Licenses are not in the lock
//! file, so the caller supplies them, for example from `cargo metadata`.

#![doc = include_str!("../README.md")]

mod license;
mod lock;
mod report;

pub use license::{Policy, license_allowed};
pub use lock::{Graph, Package, parse_lock};
pub use report::{Report, analyse};

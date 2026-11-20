//! Dependency graph analysis: cycles, license policy and transitive size.
//!
//! The graph comes from a `Cargo.lock` file or from `cargo metadata --format-version 1`
//! output. Licenses are not in the lock file, so the caller supplies them.

#![doc = include_str!("../README.md")]

mod json;
mod license;
mod lock;
mod metadata;
mod report;

pub use json::{JsonError, MAX_DEPTH, Value, parse as parse_json};
pub use license::{Policy, license_allowed};
pub use lock::{DepKind, Dependency, Graph, Package, Scope, parse_lock};
pub use metadata::{parse_metadata, parse_metadata_licenses};
pub use report::{Report, analyse};

#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod extract;
mod render;

/// File output and rustdoc injection. Needs `std`, so it is also built for
/// tests. Run it after `cargo doc`, not from `build.rs`.
#[cfg(any(feature = "std", test))]
pub mod site;
/// Compiling examples to Wasm, and the browser runner. Needs `std` for the
/// compile step, so it is also built for tests.
#[cfg(any(feature = "std", test))]
pub mod wasm;

pub use extract::{Example, Mode, doc_comments, extract};
pub use render::{escape, example_section, inject_before, page};

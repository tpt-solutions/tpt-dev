#![no_std]
#![doc = include_str!("../README.md")]
// The handle and session types hold raw C-side pointers, and move them only
// through the backend. Each `unsafe` block has a SAFETY comment.
#![allow(unsafe_code)]

#[cfg(any(test, feature = "std"))]
extern crate std;

mod handle;
mod session;

pub use handle::Handle;
pub use session::{Backend, Closed, Failed, Open, Session, Streaming};

/// A status code from the C side. Zero means success, as in most C APIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Code(pub i32);

impl Code {
    /// Success.
    pub const OK: Code = Code(0);

    /// Returns `true` for success.
    pub const fn is_ok(self) -> bool {
        self.0 == 0
    }
}

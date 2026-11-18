//! Atomics. Under `--cfg loom` these are loom's model-checked atomics.

#[cfg(loom)]
pub(crate) use loom::sync::atomic::{AtomicU32, Ordering};

#[cfg(not(loom))]
pub(crate) use core::sync::atomic::{AtomicU32, Ordering};

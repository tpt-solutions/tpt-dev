//! Atomics. Under `--cfg loom` these are loom's model-checked atomics.
//!
//! `AtomicU64` is available only where the target has 64-bit atomics, so the
//! ID generator uses `AtomicU32`. `ManualClock` is also gated on 64-bit atomics.

#[cfg(loom)]
pub(crate) use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[cfg(not(loom))]
pub(crate) use core::sync::atomic::{AtomicU32, Ordering};

#[cfg(all(not(loom), target_has_atomic = "64"))]
pub(crate) use core::sync::atomic::AtomicU64;

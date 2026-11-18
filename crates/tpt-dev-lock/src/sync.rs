//! Atomics. Under `--cfg loom` these are loom's model-checked atomics.

#[cfg(loom)]
pub(crate) use loom::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

#[cfg(not(loom))]
pub(crate) use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

/// Back-off inside a spin loop. Under loom it yields, so the model can
/// schedule the thread that holds the lock.
#[cfg(loom)]
pub(crate) fn spin_loop() {
    loom::thread::yield_now();
}

/// Back-off inside a spin loop.
#[cfg(not(loom))]
pub(crate) fn spin_loop() {
    core::hint::spin_loop();
}

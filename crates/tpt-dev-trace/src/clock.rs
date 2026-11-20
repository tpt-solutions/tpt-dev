//! Timestamp sources.

/// A source of timestamps, in whatever tick unit the device uses.
///
/// Implement it for a hardware timer, such as a DWT cycle counter or a
/// SysTick count. Any `Fn() -> u64` closure also implements it.
///
/// ```
/// use tpt_dev_trace::Clock;
///
/// let fixed = || 42u64;
/// assert_eq!(fixed.now(), 42);
/// ```
pub trait Clock {
    /// The current time in ticks. It must never go backwards.
    fn now(&self) -> u64;
}

impl<F: Fn() -> u64> Clock for F {
    fn now(&self) -> u64 {
        self()
    }
}

#[cfg(any(loom, target_has_atomic = "64"))]
pub use manual::ManualClock;

#[cfg(any(loom, target_has_atomic = "64"))]
mod manual {
    use super::Clock;
    use crate::sync::{AtomicU64, Ordering};

    /// A clock that the program sets by hand. Useful for tests and simulation.
    ///
    /// It needs 64-bit atomics, so it is not available on targets without them,
    /// such as Cortex-M4 and M7. On those targets, implement [`Clock`] over
    /// the hardware timer instead.
    ///
    /// ```
    /// use tpt_dev_trace::{Clock, ManualClock};
    ///
    /// let clock = ManualClock::new(10);
    /// clock.advance(5);
    /// assert_eq!(clock.now(), 15);
    /// ```
    pub struct ManualClock {
        ticks: AtomicU64,
    }

    impl ManualClock {
        /// Creates a clock that reads `start`.
        ///
        /// # Examples
        ///
        /// ```
        /// use tpt_dev_trace::{Clock, ManualClock};
        ///
        /// let clock = ManualClock::new(7);
        /// assert_eq!(clock.now(), 7);
        /// ```
        #[cfg(not(loom))]
        pub const fn new(start: u64) -> Self {
            Self {
                ticks: AtomicU64::new(start),
            }
        }

        /// Creates a clock that reads `start`.
        ///
        /// # Examples
        ///
        /// ```
        /// use tpt_dev_trace::{Clock, ManualClock};
        ///
        /// let clock = ManualClock::new(7);
        /// assert_eq!(clock.now(), 7);
        /// ```
        #[cfg(loom)]
        pub fn new(start: u64) -> Self {
            Self {
                ticks: AtomicU64::new(start),
            }
        }

        /// Moves the clock forward by `ticks`.
        ///
        /// # Examples
        ///
        /// ```
        /// use tpt_dev_trace::{Clock, ManualClock};
        ///
        /// let clock = ManualClock::new(0);
        /// clock.advance(3);
        /// clock.advance(4);
        /// assert_eq!(clock.now(), 7);
        /// ```
        pub fn advance(&self, ticks: u64) {
            self.ticks.fetch_add(ticks, Ordering::SeqCst);
        }
    }

    impl Clock for ManualClock {
        fn now(&self) -> u64 {
            self.ticks.load(Ordering::SeqCst)
        }
    }
}

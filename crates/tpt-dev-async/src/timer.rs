//! Delays measured in hardware ticks.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use tpt_dev_trace::Clock;

/// A future that completes once the clock reaches a deadline.
///
/// While waiting, `Delay` wakes its own task every time it is polled, so the
/// executor keeps polling it. That spends CPU while waiting. A timer-driven
/// design would sleep until the next tick instead, and this crate does not
/// provide one yet.
///
/// ```
/// use core::future::Future;
/// use core::pin::pin;
/// use core::task::{Context, Poll, Waker};
/// use tpt_dev_async::Delay;
/// use tpt_dev_trace::ManualClock;
///
/// let clock = ManualClock::new(0);
/// let mut delay = pin!(Delay::new(&clock, 3));
/// let mut cx = Context::from_waker(Waker::noop());
/// assert!(delay.as_mut().poll(&mut cx).is_pending());
/// clock.advance(3);
/// assert!(delay.as_mut().poll(&mut cx).is_ready());
/// ```
pub struct Delay<'c, C: Clock + ?Sized> {
    clock: &'c C,
    until: u64,
}

impl<'c, C: Clock + ?Sized> Delay<'c, C> {
    /// Completes after `ticks` clock ticks from now.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_async::Delay;
    /// use tpt_dev_trace::ManualClock;
    ///
    /// let clock = ManualClock::new(10);
    /// let delay = Delay::new(&clock, 5);
    /// assert_eq!(delay.deadline(), 15);
    /// ```
    pub fn new(clock: &'c C, ticks: u64) -> Self {
        Self {
            clock,
            until: clock.now().saturating_add(ticks),
        }
    }

    /// The tick at which the delay completes.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_async::Delay;
    /// use tpt_dev_trace::ManualClock;
    ///
    /// let clock = ManualClock::new(0);
    /// assert_eq!(Delay::new(&clock, 4).deadline(), 4);
    /// ```
    pub const fn deadline(&self) -> u64 {
        self.until
    }
}

impl<C: Clock + ?Sized> Future for Delay<'_, C> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.clock.now() >= this.until {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

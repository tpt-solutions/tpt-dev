//! A destination with a fixed batch buffer and a back-pressure policy.

/// Returned by a [`Sink`] that cannot take a batch right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Busy;

/// A destination for batches. Implement it for a UART, a radio, a flash
/// log, or a socket.
pub trait Sink<T> {
    /// Takes a batch, oldest first. Return [`Busy`] if the batch cannot be
    /// taken now. The lane keeps the batch and applies its [`Policy`].
    fn send(&mut self, batch: &[T]) -> Result<(), Busy>;
}

/// What a lane does with a new message when its batch is full and the sink is busy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Discard the new message. Keeps the oldest queued data.
    DropNewest,
    /// Discard the oldest queued message to make room. Keeps the freshest data.
    DropOldest,
    /// Refuse the message and tell the caller. The caller decides what to do.
    Reject,
}

/// Counters for one lane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneStats {
    /// Messages handed to the sink.
    pub sent: u64,
    /// Messages discarded by the policy.
    pub dropped: u64,
    /// Messages waiting in the batch.
    pub pending: usize,
}

/// A destination: a sink, its policy, and a fixed batch buffer of `B` messages.
///
/// A message is added to the batch. When the batch is full, the lane sends it
/// to the sink. If the sink is busy, the policy decides what happens.
///
/// ```
/// use tpt_dev_sink::{Busy, Lane, Policy, Sink};
///
/// struct Print;
/// impl Sink<u8> for Print {
///     fn send(&mut self, batch: &[u8]) -> Result<(), Busy> {
///         assert_eq!(batch.len(), 2);
///         Ok(())
///     }
/// }
///
/// let mut sink = Print;
/// let mut lane: Lane<u8, 2> = Lane::new(&mut sink, Policy::DropNewest);
/// lane.push(1).unwrap();
/// lane.push(2).unwrap(); // the batch is full and goes out now
/// assert_eq!(lane.stats().sent, 2);
/// ```
pub struct Lane<'s, T: Copy + Default, const B: usize> {
    sink: &'s mut dyn Sink<T>,
    policy: Policy,
    buf: [T; B],
    len: usize,
    sent: u64,
    dropped: u64,
}

impl<'s, T: Copy + Default, const B: usize> Lane<'s, T, B> {
    /// Creates a lane that sends to `sink` under `policy`.
    ///
    /// Fails to compile if `B` is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sink::{Busy, Lane, Policy, Sink};
    ///
    /// struct Null;
    /// impl Sink<u8> for Null {
    ///     fn send(&mut self, _batch: &[u8]) -> Result<(), Busy> {
    ///         Ok(())
    ///     }
    /// }
    ///
    /// let mut sink = Null;
    /// let lane: Lane<u8, 3> = Lane::new(&mut sink, Policy::Reject);
    /// assert_eq!(lane.stats().pending, 0);
    /// ```
    pub fn new(sink: &'s mut dyn Sink<T>, policy: Policy) -> Self {
        const { assert!(B > 0, "a lane needs room for at least one message") };
        Self {
            sink,
            policy,
            buf: [T::default(); B],
            len: 0,
            sent: 0,
            dropped: 0,
        }
    }

    /// Adds a message to the batch.
    ///
    /// Returns `Err(Busy)` only with [`Policy::Reject`], when the batch is full
    /// and the sink refuses it. With the drop policies it always returns `Ok`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sink::{Busy, Lane, Policy, Sink};
    ///
    /// struct Blocked;
    /// impl Sink<u8> for Blocked {
    ///     fn send(&mut self, _batch: &[u8]) -> Result<(), Busy> {
    ///         Err(Busy)
    ///     }
    /// }
    ///
    /// let mut sink = Blocked;
    /// let mut lane: Lane<u8, 1> = Lane::new(&mut sink, Policy::Reject);
    /// // The first message fills the batch, and the blocked sink keeps it.
    /// assert!(lane.push(1).is_ok());
    /// // The batch is still full, so the Reject policy refuses the next message.
    /// assert_eq!(lane.push(2), Err(Busy));
    /// ```
    pub fn push(&mut self, value: T) -> Result<(), Busy> {
        if self.len == B {
            // An earlier send was refused and the batch is still full.
            if self.send_batch().is_err() {
                match self.policy {
                    Policy::DropNewest => {
                        self.dropped = self.dropped.saturating_add(1);
                        return Ok(());
                    }
                    Policy::DropOldest => {
                        self.buf.copy_within(1.., 0);
                        self.len = B - 1;
                        self.dropped = self.dropped.saturating_add(1);
                    }
                    Policy::Reject => return Err(Busy),
                }
            }
        }
        self.buf[self.len] = value;
        self.len += 1;
        if self.len == B {
            // Send as soon as the batch is full. If the sink refuses, the
            // batch stays full and the policy applies to the next push.
            let _ = self.send_batch();
        }
        Ok(())
    }

    /// Sends any partly filled batch now. Returns `Err(Busy)` if the sink refuses it.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sink::{Busy, Lane, Policy, Sink};
    ///
    /// struct Blocked;
    /// impl Sink<u8> for Blocked {
    ///     fn send(&mut self, _batch: &[u8]) -> Result<(), Busy> {
    ///         Err(Busy)
    ///     }
    /// }
    ///
    /// let mut sink = Blocked;
    /// let mut lane: Lane<u8, 2> = Lane::new(&mut sink, Policy::DropNewest);
    /// lane.push(1).unwrap();
    /// assert_eq!(lane.flush(), Err(Busy));
    /// assert_eq!(lane.stats().pending, 1);
    /// ```
    pub fn flush(&mut self) -> Result<(), Busy> {
        self.send_batch()
    }

    /// The lane's counters.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sink::{Busy, Lane, Policy, Sink};
    ///
    /// struct Null;
    /// impl Sink<u8> for Null {
    ///     fn send(&mut self, _batch: &[u8]) -> Result<(), Busy> {
    ///         Ok(())
    ///     }
    /// }
    ///
    /// let mut sink = Null;
    /// let mut lane: Lane<u8, 2> = Lane::new(&mut sink, Policy::DropNewest);
    /// lane.push(1).unwrap();
    /// assert_eq!(lane.stats().pending, 1);
    /// lane.push(2).unwrap();
    /// let stats = lane.stats();
    /// assert_eq!((stats.sent, stats.pending), (2, 0));
    /// ```
    pub const fn stats(&self) -> LaneStats {
        LaneStats {
            sent: self.sent,
            dropped: self.dropped,
            pending: self.len,
        }
    }

    fn send_batch(&mut self) -> Result<(), Busy> {
        if self.len == 0 {
            return Ok(());
        }
        self.sink.send(&self.buf[..self.len])?;
        self.sent = self.sent.saturating_add(self.len as u64);
        self.len = 0;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use core::cell::Cell;
    use std::vec::Vec;

    /// A sink that records batches. `busy` is shared so a test can change it
    /// while the lane still holds the sink.
    struct Probe<'a> {
        busy: &'a Cell<bool>,
        batches: Vec<Vec<u8>>,
    }

    impl Sink<u8> for Probe<'_> {
        fn send(&mut self, batch: &[u8]) -> Result<(), Busy> {
            if self.busy.get() {
                return Err(Busy);
            }
            self.batches.push(batch.to_vec());
            Ok(())
        }
    }

    #[test]
    fn full_batch_goes_out_at_capacity() {
        let busy = Cell::new(false);
        let mut p = Probe {
            busy: &busy,
            batches: Vec::new(),
        };
        let mut lane: Lane<u8, 3> = Lane::new(&mut p, Policy::DropNewest);
        for v in 1..=5 {
            lane.push(v).unwrap();
        }
        assert_eq!(
            lane.stats(),
            LaneStats {
                sent: 3,
                dropped: 0,
                pending: 2
            }
        );
        lane.flush().unwrap();
        assert_eq!(p.batches, [std::vec![1, 2, 3], std::vec![4, 5]]);
    }

    #[test]
    fn drop_newest_keeps_queued_data() {
        let busy = Cell::new(true);
        let mut p = Probe {
            busy: &busy,
            batches: Vec::new(),
        };
        let mut lane: Lane<u8, 2> = Lane::new(&mut p, Policy::DropNewest);
        for v in 1..=4 {
            assert!(lane.push(v).is_ok());
        }
        // The batch held 1 and 2; 3 and 4 were dropped.
        let s = lane.stats();
        assert_eq!((s.sent, s.dropped, s.pending), (0, 2, 2));
    }

    #[test]
    fn drop_oldest_keeps_fresh_data() {
        let busy = Cell::new(true);
        let mut p = Probe {
            busy: &busy,
            batches: Vec::new(),
        };
        let mut lane: Lane<u8, 2> = Lane::new(&mut p, Policy::DropOldest);
        for v in 1..=4 {
            assert!(lane.push(v).is_ok());
        }
        // The batch now holds the two newest: 3 and 4.
        busy.set(false);
        lane.flush().unwrap();
        assert_eq!(lane.stats().dropped, 2);
        assert_eq!(p.batches, [std::vec![3, 4]]);
    }

    #[test]
    fn reject_reports_busy_to_caller() {
        let busy = Cell::new(true);
        let mut p = Probe {
            busy: &busy,
            batches: Vec::new(),
        };
        let mut lane: Lane<u8, 1> = Lane::new(&mut p, Policy::Reject);
        assert!(lane.push(1).is_ok());
        assert_eq!(lane.push(2), Err(Busy));
        assert_eq!(lane.stats().pending, 1);
    }
}

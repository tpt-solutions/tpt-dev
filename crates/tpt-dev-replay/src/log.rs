//! Fixed-size event logs.
//!
//! [`Log`] keeps every event and refuses new ones when full, so a run can be
//! replayed. [`Ring`] keeps the most recent events and counts what it dropped,
//! which suits long-running systems where only the latest history matters.

use crate::event::Event;

/// Returned by [`Log::push`] when the log is full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Full;

/// An append-only log of up to `N` events.
///
/// ```
/// use tpt_dev_replay::{Event, Log};
///
/// let mut log: Log<2> = Log::new();
/// log.push(Event::Schedule { slot: 0 }).unwrap();
/// assert_eq!(log.as_slice(), &[Event::Schedule { slot: 0 }]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Log<const N: usize> {
    buf: [Event; N],
    len: usize,
}

impl<const N: usize> Default for Log<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Log<N> {
    /// An empty log.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::Log;
    ///
    /// let log: Log<4> = Log::new();
    /// assert!(log.is_empty());
    /// ```
    pub const fn new() -> Self {
        Self {
            buf: [Event::Schedule { slot: 0 }; N],
            len: 0,
        }
    }

    /// Appends an event, or returns [`Full`] without storing it.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::{Event, Full, Log};
    ///
    /// let mut log: Log<1> = Log::new();
    /// assert!(log.push(Event::Schedule { slot: 0 }).is_ok());
    /// // The log holds one event, so the second is refused.
    /// assert_eq!(log.push(Event::Schedule { slot: 1 }), Err(Full));
    /// ```
    pub fn push(&mut self, e: Event) -> Result<(), Full> {
        if self.len == N {
            return Err(Full);
        }
        self.buf[self.len] = e;
        self.len += 1;
        Ok(())
    }

    /// The events, oldest first.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::{Event, Log};
    ///
    /// let mut log: Log<4> = Log::new();
    /// log.push(Event::Interrupt { slot: 2 }).unwrap();
    /// assert_eq!(log.as_slice(), &[Event::Interrupt { slot: 2 }]);
    /// ```
    pub fn as_slice(&self) -> &[Event] {
        &self.buf[..self.len]
    }

    /// Number of events stored.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::{Event, Log};
    ///
    /// let mut log: Log<4> = Log::new();
    /// assert_eq!(log.len(), 0);
    /// log.push(Event::Schedule { slot: 0 }).unwrap();
    /// assert_eq!(log.len(), 1);
    /// ```
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` when no events are stored.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::Log;
    ///
    /// let log: Log<2> = Log::new();
    /// assert!(log.is_empty());
    /// ```
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The capacity, `N`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::Log;
    ///
    /// let log: Log<8> = Log::new();
    /// assert_eq!(log.capacity(), 8);
    /// ```
    pub const fn capacity(&self) -> usize {
        N
    }
}

/// A ring buffer of the most recent `N` events.
///
/// When full, a new event overwrites the oldest, and [`Ring::overwritten`]
/// counts the loss. A ring that has overwritten events cannot be replayed in full.
///
/// ```
/// use tpt_dev_replay::{Event, Ring};
///
/// let mut ring: Ring<2> = Ring::new();
/// for slot in 0..3 {
///     ring.push(Event::Schedule { slot });
/// }
/// assert_eq!(ring.overwritten(), 1);
/// assert_eq!(ring.iter().next(), Some(Event::Schedule { slot: 1 }));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ring<const N: usize> {
    buf: [Event; N],
    start: usize,
    len: usize,
    overwritten: u64,
}

impl<const N: usize> Default for Ring<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Ring<N> {
    /// An empty ring. Fails to compile if `N` is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::Ring;
    ///
    /// let ring: Ring<3> = Ring::new();
    /// assert_eq!(ring.iter().count(), 0);
    /// assert_eq!(ring.overwritten(), 0);
    /// ```
    pub const fn new() -> Self {
        const { assert!(N > 0, "a ring needs at least one slot") };
        Self {
            buf: [Event::Schedule { slot: 0 }; N],
            start: 0,
            len: 0,
            overwritten: 0,
        }
    }

    /// Adds an event, overwriting the oldest one if the ring is full.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::{Event, Ring};
    ///
    /// let mut ring: Ring<2> = Ring::new();
    /// ring.push(Event::Schedule { slot: 0 });
    /// ring.push(Event::Schedule { slot: 1 });
    /// ring.push(Event::Schedule { slot: 2 });
    /// assert_eq!(ring.iter().count(), 2);
    /// ```
    pub fn push(&mut self, e: Event) {
        if self.len == N {
            self.buf[self.start] = e;
            self.start = (self.start + 1) % N;
            self.overwritten = self.overwritten.saturating_add(1);
        } else {
            self.buf[(self.start + self.len) % N] = e;
            self.len += 1;
        }
    }

    /// The stored events, oldest first.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::{Event, Ring};
    ///
    /// let mut ring: Ring<4> = Ring::new();
    /// ring.push(Event::Schedule { slot: 0 });
    /// ring.push(Event::Interrupt { slot: 1 });
    /// let events: Vec<Event> = ring.iter().collect();
    /// assert_eq!(events, vec![Event::Schedule { slot: 0 }, Event::Interrupt { slot: 1 }]);
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        (0..self.len).map(move |i| self.buf[(self.start + i) % N])
    }

    /// Number of events lost to overwriting.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::{Event, Ring};
    ///
    /// let mut ring: Ring<2> = Ring::new();
    /// for slot in 0..5 {
    ///     ring.push(Event::Schedule { slot });
    /// }
    /// // Five pushes into two slots lose three events.
    /// assert_eq!(ring.overwritten(), 3);
    /// ```
    pub const fn overwritten(&self) -> u64 {
        self.overwritten
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn log_refuses_when_full() {
        let mut log: Log<1> = Log::new();
        assert!(log.push(Event::Interrupt { slot: 1 }).is_ok());
        assert_eq!(log.push(Event::Interrupt { slot: 2 }), Err(Full));
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn ring_keeps_newest_in_order() {
        let mut ring: Ring<3> = Ring::new();
        for slot in 0..5 {
            ring.push(Event::Schedule { slot });
        }
        let got: std::vec::Vec<Event> = ring.iter().collect();
        assert_eq!(
            got,
            [
                Event::Schedule { slot: 2 },
                Event::Schedule { slot: 3 },
                Event::Schedule { slot: 4 }
            ]
        );
        assert_eq!(ring.overwritten(), 2);
    }
}

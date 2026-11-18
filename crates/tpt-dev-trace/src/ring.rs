//! A fixed-capacity ring buffer of records.

use crate::record::Record;

/// A ring buffer holding up to `N` records.
///
/// When the buffer is full, a new record overwrites the oldest one, and the
/// drop counter goes up. Recording never blocks and never allocates. The
/// counter tells the reader how much data was lost.
///
/// ```
/// use tpt_dev_trace::{Kind, Record, Ring};
///
/// let mut ring: Ring<2> = Ring::new();
/// let r = Record { kind: Kind::Event, name: "tick", ..Record::EMPTY };
/// ring.push(r);
/// ring.push(r);
/// ring.push(r); // overwrites the oldest record
/// assert_eq!(ring.len(), 2);
/// assert_eq!(ring.dropped(), 1);
/// assert!(ring.pop().is_some());
/// ```
pub struct Ring<const N: usize> {
    buf: [Record; N],
    start: usize,
    len: usize,
    dropped: u64,
}

impl<const N: usize> Ring<N> {
    /// Creates an empty ring. Fails to compile if `N` is zero.
    pub const fn new() -> Self {
        const { assert!(N > 0, "a ring needs at least one slot") };
        Self {
            buf: [Record::EMPTY; N],
            start: 0,
            len: 0,
            dropped: 0,
        }
    }

    /// Adds a record. If the ring is full, the oldest record is overwritten.
    pub fn push(&mut self, record: Record) {
        if self.len == N {
            self.buf[self.start] = record;
            self.start = (self.start + 1) % N;
            self.dropped = self.dropped.saturating_add(1);
        } else {
            self.buf[(self.start + self.len) % N] = record;
            self.len += 1;
        }
    }

    /// Removes and returns the oldest record.
    pub fn pop(&mut self) -> Option<Record> {
        if self.len == 0 {
            return None;
        }
        let record = self.buf[self.start];
        self.start = (self.start + 1) % N;
        self.len -= 1;
        Some(record)
    }

    /// Number of records waiting to be read.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` when there are no records.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The capacity, `N`.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Number of records overwritten before they were read.
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }
}

impl<const N: usize> Default for Ring<N> {
    fn default() -> Self {
        Self::new()
    }
}

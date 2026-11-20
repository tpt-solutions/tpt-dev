//! The shared collector and the export interface.

use tpt_dev_lock::{L1, Mutex, Token};

use crate::record::Record;
use crate::ring::Ring;

/// Destination for exported records. Implement it for a UART, a flash log,
/// a network socket, or an in-memory buffer in tests.
pub trait Sink {
    /// Receives a batch of records, oldest first. The slice is valid only
    /// for this call.
    fn export(&mut self, records: &[Record]);
}

/// Records are batched this many at a time when exporting.
const BATCH: usize = 16;

/// A ring of records that many tasks can write to at once.
///
/// The ring sits behind a `tpt-dev-lock` mutex at level 1. Recording takes
/// the lock only for a single push, so the critical section is short. The
/// lock is a spin lock. Do not record from an interrupt handler while the
/// same collector is used from thread context, because a spin lock held by
/// the interrupted code would never be released.
pub struct Collector<const N: usize> {
    ring: Mutex<L1, Ring<N>>,
}

impl<const N: usize> Collector<N> {
    /// Creates an empty collector.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{Collector, Sink};
    ///
    /// let c: Collector<4> = Collector::new();
    /// assert!(c.is_empty());
    /// ```
    #[cfg(not(loom))]
    pub const fn new() -> Self {
        Self {
            ring: Mutex::new(Ring::new()),
        }
    }

    /// Creates an empty collector.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{Collector, Sink};
    ///
    /// let c: Collector<4> = Collector::new();
    /// assert!(c.is_empty());
    /// ```
    #[cfg(loom)]
    pub fn new() -> Self {
        Self {
            ring: Mutex::new(Ring::new()),
        }
    }

    /// Adds a record. Drops the oldest record if the ring is full.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{Collector, Kind, Record};
    ///
    /// let c: Collector<4> = Collector::new();
    /// c.record(Record { kind: Kind::Event, name: "tick", ..Record::EMPTY });
    /// assert_eq!(c.len(), 1);
    /// ```
    pub fn record(&self, record: Record) {
        self.ring
            .lock_with(Token::root(), |ring, _| ring.push(record));
    }

    /// Number of records waiting to be exported.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{Collector, Kind, Record};
    ///
    /// let c: Collector<4> = Collector::new();
    /// assert_eq!(c.len(), 0);
    /// c.record(Record { kind: Kind::Event, name: "tick", ..Record::EMPTY });
    /// assert_eq!(c.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.ring.lock_with(Token::root(), |ring, _| ring.len())
    }

    /// Returns `true` when there is nothing to export.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::Collector;
    ///
    /// let c: Collector<4> = Collector::new();
    /// assert!(c.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of records lost to overwriting so far.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{Collector, Kind, Record};
    ///
    /// let c: Collector<2> = Collector::new();
    /// let r = Record { kind: Kind::Event, name: "tick", ..Record::EMPTY };
    /// for _ in 0..3 {
    ///     c.record(r);
    /// }
    /// assert_eq!(c.len(), 2);
    /// assert_eq!(c.dropped(), 1);
    /// ```
    pub fn dropped(&self) -> u64 {
        self.ring.lock_with(Token::root(), |ring, _| ring.dropped())
    }

    /// Exports every waiting record to `sink`, oldest first, and empties the collector.
    ///
    /// Records are taken from the ring in batches. The lock is not held
    /// while `sink` runs, so recording can continue during the export.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{Collector, Kind, Record, Sink};
    ///
    /// struct Keep(Vec<Record>);
    /// impl Sink for Keep {
    ///     fn export(&mut self, records: &[Record]) {
    ///         self.0.extend_from_slice(records);
    ///     }
    /// }
    ///
    /// let c: Collector<4> = Collector::new();
    /// c.record(Record { kind: Kind::Event, name: "a", ..Record::EMPTY });
    /// c.record(Record { kind: Kind::Event, name: "b", ..Record::EMPTY });
    /// let mut keep = Keep(Vec::new());
    /// c.flush(&mut keep);
    /// assert_eq!(keep.0.len(), 2);
    /// assert_eq!(keep.0[0].name, "a");
    /// assert!(c.is_empty());
    /// ```
    pub fn flush<S: Sink>(&self, sink: &mut S) {
        let mut batch = [Record::EMPTY; BATCH];
        loop {
            let n = self.ring.lock_with(Token::root(), |ring, _| {
                let mut n = 0;
                while n < BATCH {
                    match ring.pop() {
                        Some(r) => {
                            batch[n] = r;
                            n += 1;
                        }
                        None => break,
                    }
                }
                n
            });
            if n == 0 {
                break;
            }
            sink.export(&batch[..n]);
        }
    }
}

impl<const N: usize> Default for Collector<N> {
    fn default() -> Self {
        Self::new()
    }
}

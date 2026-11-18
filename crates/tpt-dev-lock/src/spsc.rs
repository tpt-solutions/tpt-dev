//! A lock-free, fixed-capacity, single-producer single-consumer queue.
//!
//! The queue stores its items in an inline array. It never allocates and
//! never blocks. Both `push` and `pop` take a bounded number of steps.

use core::mem::MaybeUninit;

use crate::cell::UnsafeCell;
use crate::sync::{AtomicUsize, Ordering};

/// A ring buffer of capacity `N`. `N` must be a non-zero power of two.
///
/// ```
/// use tpt_dev_lock::Spsc;
///
/// let mut q: Spsc<u8, 4> = Spsc::new();
/// let (mut tx, mut rx) = q.split();
/// tx.push(1).unwrap();
/// tx.push(2).unwrap();
/// assert_eq!(rx.pop(), Some(1));
/// assert_eq!(rx.pop(), Some(2));
/// assert_eq!(rx.pop(), None);
/// ```
pub struct Spsc<T, const N: usize> {
    buf: [UnsafeCell<MaybeUninit<T>>; N],
    /// Next slot to read. Written only by the consumer.
    head: AtomicUsize,
    /// Next slot to write. Written only by the producer.
    tail: AtomicUsize,
}

// SAFETY: the producer writes slots only between `tail` and `head + N`, and the
// consumer reads only slots before `tail`. The release/acquire pairs on `head`
// and `tail` order those accesses. Items move between threads, so `T: Send`.
unsafe impl<T: Send, const N: usize> Sync for Spsc<T, N> {}

impl<T, const N: usize> Spsc<T, N> {
    /// Creates an empty queue.
    ///
    /// Fails to compile unless `N` is a non-zero power of two.
    #[cfg(not(loom))]
    pub const fn new() -> Self {
        const {
            assert!(
                N > 0 && N.is_power_of_two(),
                "capacity must be a non-zero power of two"
            )
        };
        Self {
            buf: [const { UnsafeCell::new(MaybeUninit::uninit()) }; N],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Creates an empty queue.
    #[cfg(loom)]
    pub fn new() -> Self {
        const {
            assert!(
                N > 0 && N.is_power_of_two(),
                "capacity must be a non-zero power of two"
            )
        };
        Self {
            buf: core::array::from_fn(|_| UnsafeCell::new(MaybeUninit::uninit())),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// The fixed capacity of the queue.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Returns the producer and consumer handles.
    pub fn split(&mut self) -> (Producer<'_, T, N>, Consumer<'_, T, N>) {
        (Producer { q: self }, Consumer { q: self })
    }
}

impl<T, const N: usize> Default for Spsc<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Drop for Spsc<T, N> {
    fn drop(&mut self) {
        // Drop the items that were pushed but not popped.
        let mut head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        while head != tail {
            // SAFETY: `&mut self` means no other handle is alive. Slots in
            // `head..tail` hold initialised items that were never popped.
            self.buf[head & (N - 1)].with_mut(|ptr| unsafe { (*ptr).assume_init_drop() });
            head = head.wrapping_add(1);
        }
    }
}

/// The producing half of a [`Spsc`].
pub struct Producer<'a, T, const N: usize> {
    q: &'a Spsc<T, N>,
}

impl<T, const N: usize> Producer<'_, T, N> {
    /// Pushes `value`, or returns it back if the queue is full.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        let tail = self.q.tail.load(Ordering::Relaxed);
        // Acquire: the consumer has finished reading the slot we are about to reuse.
        let head = self.q.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) == N {
            return Err(value);
        }
        // SAFETY: the slot at `tail` is outside `head..tail`, so the consumer
        // is not reading it, and the producer owns it until `tail` is published.
        self.q.buf[tail & (N - 1)].with_mut(|ptr| unsafe { (*ptr).write(value) });
        // Release: publishes the write to the consumer.
        self.q.tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// The fixed capacity of the queue.
    pub const fn capacity(&self) -> usize {
        N
    }
}

/// The consuming half of a [`Spsc`].
pub struct Consumer<'a, T, const N: usize> {
    q: &'a Spsc<T, N>,
}

impl<T, const N: usize> Consumer<'_, T, N> {
    /// Pops the oldest item, or returns `None` if the queue is empty.
    pub fn pop(&mut self) -> Option<T> {
        let head = self.q.head.load(Ordering::Relaxed);
        // Acquire: the producer's write to the slot at `head` is visible.
        let tail = self.q.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        // SAFETY: the slot at `head` is inside `head..tail`, so the producer has
        // initialised it and is not touching it. The consumer owns it until `head` is published.
        let value = self.q.buf[head & (N - 1)].with(|ptr| unsafe { (*ptr).assume_init_read() });
        // Release: tells the producer the slot can be reused.
        self.q.head.store(head.wrapping_add(1), Ordering::Release);
        Some(value)
    }
}

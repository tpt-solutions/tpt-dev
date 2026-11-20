//! A wait-free single-writer register: a triple buffer.
//!
//! The writer and the reader never block each other and never retry. Each
//! `write` and `with_latest` takes a fixed number of steps, so both are
//! wait-free. The reader sees the most recent value the writer has published
//! when it looks. Values can be skipped, but a value is never torn, and the
//! reader never sees an older value after a newer one.

use crate::cell::UnsafeCell;
use crate::sync::{AtomicU8, Ordering};

/// Low two bits: index of the middle slot. Bit 2: a new value is waiting.
const INDEX_MASK: u8 = 0b011;
const DIRTY: u8 = 0b100;

/// Three slots shared between one writer and one reader.
///
/// Call [`TripleBuffer::split`] to get the two handles. Splitting takes
/// `&mut self`, so there is only ever one writer and one reader.
///
/// ```
/// use tpt_dev_lock::TripleBuffer;
///
/// let mut buf = TripleBuffer::new(0u32, 0, 0);
/// let (mut writer, mut reader) = buf.split();
/// writer.write(7);
/// reader.with_latest(|v| assert_eq!(*v, 7));
/// ```
pub struct TripleBuffer<T> {
    slots: [UnsafeCell<T>; 3],
    state: AtomicU8,
}

// SAFETY: each slot is touched only by the one party that owns its index. The
// index moves between parties with `state`, which uses acquire/release.
// Values move between threads, so `T: Send` is required.
unsafe impl<T: Send> Sync for TripleBuffer<T> {}

impl<T> TripleBuffer<T> {
    /// Creates a buffer with three initial slot values.
    #[cfg(not(loom))]
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lock::TripleBuffer;
    ///
    /// let mut buf = TripleBuffer::new(0u8, 0, 0);
    /// let (mut writer, mut reader) = buf.split();
    /// writer.write(5);
    /// reader.with_latest(|value| assert_eq!(*value, 5));
    /// ```
    pub const fn new(a: T, b: T, c: T) -> Self {
        Self {
            slots: [UnsafeCell::new(a), UnsafeCell::new(b), UnsafeCell::new(c)],
            state: AtomicU8::new(1),
        }
    }

    /// Creates a buffer with three initial slot values.
    #[cfg(loom)]
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lock::TripleBuffer;
    ///
    /// let mut buf = TripleBuffer::new(0u8, 0, 0);
    /// let (mut writer, mut reader) = buf.split();
    /// writer.write(5);
    /// reader.with_latest(|value| assert_eq!(*value, 5));
    /// ```
    pub fn new(a: T, b: T, c: T) -> Self {
        Self {
            slots: [UnsafeCell::new(a), UnsafeCell::new(b), UnsafeCell::new(c)],
            state: AtomicU8::new(1),
        }
    }

    /// Returns the writer and reader handles.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lock::TripleBuffer;
    ///
    /// let mut buf = TripleBuffer::new(1u8, 2, 3);
    /// let (_writer, mut reader) = buf.split();
    /// reader.with_latest(|value| assert_eq!(*value, 3));
    /// ```
    pub fn split(&mut self) -> (Writer<'_, T>, Reader<'_, T>) {
        (
            Writer { buf: self, back: 0 },
            Reader {
                buf: self,
                front: 2,
            },
        )
    }
}

/// The writing half of a [`TripleBuffer`].
pub struct Writer<'a, T> {
    buf: &'a TripleBuffer<T>,
    back: u8,
}

impl<T> Writer<'_, T> {
    /// Publishes `value`. The previous unread value, if any, is dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lock::TripleBuffer;
    ///
    /// let mut buf = TripleBuffer::new(0u8, 0, 0);
    /// let (mut writer, mut reader) = buf.split();
    /// writer.write(42);
    /// reader.with_latest(|value| assert_eq!(*value, 42));
    /// ```
    pub fn write(&mut self, value: T) {
        // SAFETY: `back` is owned by the writer, so no other party touches this slot.
        self.buf.slots[usize::from(self.back)].with_mut(|ptr| unsafe { *ptr = value });
        // Swap in our slot as the middle one and take back the old middle.
        // AcqRel: the reader's last access to the slot we now own happens before this.
        let old = self.buf.state.swap(self.back | DIRTY, Ordering::AcqRel);
        self.back = old & INDEX_MASK;
    }
}

/// The reading half of a [`TripleBuffer`].
pub struct Reader<'a, T> {
    buf: &'a TripleBuffer<T>,
    front: u8,
}

impl<T> Reader<'_, T> {
    /// Calls `f` with the latest published value.
    ///
    /// If nothing new was written since the last call, `f` sees the same
    /// value again.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lock::TripleBuffer;
    ///
    /// let mut buf = TripleBuffer::new(0u8, 0, 0);
    /// let (mut writer, mut reader) = buf.split();
    /// writer.write(1);
    /// assert_eq!(reader.with_latest(|value| *value), 1);
    /// // Nothing new was written, so the same value is seen again.
    /// assert_eq!(reader.with_latest(|value| *value), 1);
    /// ```
    pub fn with_latest<R>(&mut self, f: impl FnOnce(&T) -> R) -> R {
        if self.buf.state.load(Ordering::Relaxed) & DIRTY != 0 {
            // Take the middle slot and leave our old slot as the middle one.
            // AcqRel: the writer's write to the slot we take happens before this.
            let old = self.buf.state.swap(self.front, Ordering::AcqRel);
            self.front = old & INDEX_MASK;
        }
        // SAFETY: `front` is owned by the reader, so no other party touches this slot.
        self.buf.slots[usize::from(self.front)].with(|ptr| f(unsafe { &*ptr }))
    }
}

//! A spin mutex whose lock order is checked at compile time.

use core::marker::PhantomData;

use crate::cell::UnsafeCell;
use crate::order::{Above, Token};
use crate::sync::{AtomicBool, Ordering, spin_loop};

/// A spin mutex at lock level `L`.
///
/// The value is reached only through [`Mutex::lock_with`]. The closure gets a
/// [`Token`] for level `L`, which is the only way to take locks nested
/// inside this one. Because nesting follows the levels, two threads cannot
/// take the same pair of locks in opposite orders, so this type cannot
/// deadlock with itself.
///
/// The lock uses spinning, so it suits short critical sections. It does not
/// block, and it does not sleep.
///
/// ```
/// use tpt_dev_lock::{L1, L2, Mutex, Token};
///
/// let outer: Mutex<L1, u32> = Mutex::new(1);
/// let inner: Mutex<L2, u32> = Mutex::new(10);
///
/// let sum = outer.lock_with(Token::root(), |a, held| {
///     inner.lock_with(held, |b, _| *a + *b)
/// });
/// assert_eq!(sum, 11);
/// ```
///
/// Taking them in the opposite order does not compile:
///
/// ```compile_fail
/// use tpt_dev_lock::{L1, L2, Mutex, Token};
///
/// let outer: Mutex<L1, u32> = Mutex::new(1);
/// let inner: Mutex<L2, u32> = Mutex::new(10);
/// inner.lock_with(Token::root(), |_, held| outer.lock_with(held, |_, _| ()));
/// ```
pub struct Mutex<L, T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
    _level: PhantomData<L>,
}

// SAFETY: access to `value` is serialised by `locked`. Sending the mutex to
// another thread moves the value, which needs `T: Send`.
unsafe impl<L, T: Send> Sync for Mutex<L, T> {}

impl<L, T> Mutex<L, T> {
    /// Creates a mutex holding `value`.
    #[cfg(not(loom))]
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lock::{L1, Mutex, Token};
    ///
    /// let counter: Mutex<L1, u32> = Mutex::new(0);
    /// counter.lock_with(Token::root(), |value, _| *value += 1);
    /// let seen = counter.lock_with(Token::root(), |value, _| *value);
    /// assert_eq!(seen, 1);
    /// ```
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
            _level: PhantomData,
        }
    }

    /// Creates a mutex holding `value`.
    #[cfg(loom)]
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lock::{L1, Mutex, Token};
    ///
    /// let counter: Mutex<L1, u32> = Mutex::new(0);
    /// counter.lock_with(Token::root(), |value, _| *value += 1);
    /// let seen = counter.lock_with(Token::root(), |value, _| *value);
    /// assert_eq!(seen, 1);
    /// ```
    pub fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
            _level: PhantomData,
        }
    }

    /// Runs `f` with exclusive access to the value.
    ///
    /// `held` is the token of the caller's current lock level. The compiler
    /// requires `L` to be [`Above`] that level. `f` receives a token for `L`
    /// for taking locks nested inside this one. The lock is released when
    /// `f` returns, including when it unwinds.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lock::{L1, L2, Mutex, Token};
    ///
    /// let outer: Mutex<L1, u32> = Mutex::new(1);
    /// let inner: Mutex<L2, u32> = Mutex::new(10);
    /// let sum = outer.lock_with(Token::root(), |a, held| inner.lock_with(held, |b, _| *a + *b));
    /// assert_eq!(sum, 11);
    /// ```
    pub fn lock_with<Held, R>(&self, held: Token<Held>, f: impl FnOnce(&mut T, Token<L>) -> R) -> R
    where
        L: Above<Held>,
    {
        let _ = held;
        self.acquire();
        let _release = Release(&self.locked);
        // SAFETY: `acquire` gave this thread exclusive access until `_release`
        // drops, and no other reference to `value` exists meanwhile.
        self.value
            .with_mut(|ptr| f(unsafe { &mut *ptr }, Token::new()))
    }

    fn acquire(&self) {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spin_loop();
        }
    }
}

struct Release<'a>(&'a AtomicBool);

impl Drop for Release<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

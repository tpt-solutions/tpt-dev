//! Compile-time lock ordering.
//!
//! Every lock has a level, written as a type. Levels are built from [`Nil`]
//! and [`Succ`], and [`Above`] says one level is strictly higher than another.
//! A [`Token`] records the highest level currently held. A lock can be taken
//! only when its level is [`Above`] that token's level, so the compiler
//! rejects any nesting that goes against the order. Taking a lock at the
//! same level twice, which would deadlock a non-reentrant lock, is rejected
//! the same way.

use core::marker::PhantomData;

/// The lowest level. No lock has this level, and every lock is above it.
pub struct Nil;

/// The level one above `N`.
pub struct Succ<N>(PhantomData<N>);

/// `Self` is strictly above `Lower`.
///
/// This trait is implemented by the crate only. Use it in bounds, not as a
/// base for your own levels.
pub trait Above<Lower> {}

impl<A> Above<Nil> for Succ<A> {}

impl<A, B> Above<Succ<B>> for Succ<A> where A: Above<B> {}

/// Level 1.
pub type L1 = Succ<Nil>;
/// Level 2.
pub type L2 = Succ<L1>;
/// Level 3.
pub type L3 = Succ<L2>;
/// Level 4.
pub type L4 = Succ<L3>;
/// Level 5.
pub type L5 = Succ<L4>;
/// Level 6.
pub type L6 = Succ<L5>;
/// Level 7.
pub type L7 = Succ<L6>;
/// Level 8.
pub type L8 = Succ<L7>;

/// Proof that the caller holds no lock above level `L`.
///
/// Tokens are zero-sized and `Copy`. They are not a proof that a lock is
/// held. Locks are released when the closure passed to `lock_with` returns,
/// so the nesting of those closures is what limits the order.
pub struct Token<L>(PhantomData<L>);

impl<L> Token<L> {
    pub(crate) const fn new() -> Self {
        Self(PhantomData)
    }
}

impl Token<Nil> {
    /// The token for a thread that holds no locks.
    pub const fn root() -> Self {
        Self::new()
    }
}

impl<L> Clone for Token<L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L> Copy for Token<L> {}

impl<L> core::fmt::Debug for Token<L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Token")
    }
}

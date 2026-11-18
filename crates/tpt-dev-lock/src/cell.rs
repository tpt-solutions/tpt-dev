//! `UnsafeCell` wrapper. Under `--cfg loom` it is loom's model-checked cell,
//! so loom tests see every access. Otherwise it is `core::cell::UnsafeCell`.

#[cfg(loom)]
pub(crate) use loom::cell::UnsafeCell;

#[cfg(not(loom))]
pub(crate) use self::std_cell::UnsafeCell;

#[cfg(not(loom))]
mod std_cell {
    /// Interior-mutability cell with the same `with`/`with_mut` API as loom's.
    pub(crate) struct UnsafeCell<T>(core::cell::UnsafeCell<T>);

    impl<T> UnsafeCell<T> {
        pub(crate) const fn new(value: T) -> Self {
            Self(core::cell::UnsafeCell::new(value))
        }

        pub(crate) fn with<R>(&self, f: impl FnOnce(*const T) -> R) -> R {
            f(self.0.get())
        }

        pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
            f(self.0.get())
        }
    }
}

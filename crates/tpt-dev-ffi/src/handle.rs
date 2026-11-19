//! Opaque handles: non-null, typed pointers to C-side objects.

use core::fmt;
use core::ptr::NonNull;

/// A non-null pointer to an opaque C-side object of type `T`.
///
/// The type parameter says what the object is, so a `Handle<Socket>` cannot
/// be passed where a `Handle<File>` is expected. The handle itself does not
/// own the object. Ownership is handled by [`crate::Session`].
///
/// ```
/// use tpt_dev_ffi::Handle;
///
/// struct Counter;
/// let mut value = Counter;
/// let h = Handle::new(&mut value as *mut Counter).unwrap();
/// assert!(Handle::<Counter>::new(core::ptr::null_mut()).is_none());
/// # let _ = h;
/// ```
pub struct Handle<T> {
    ptr: NonNull<T>,
}

impl<T> Handle<T> {
    /// Wraps a raw pointer, or returns `None` for null.
    ///
    /// The caller guarantees that a non-null pointer refers to a live object of type `T`.
    pub fn new(ptr: *mut T) -> Option<Self> {
        NonNull::new(ptr).map(|ptr| Self { ptr })
    }

    /// The raw pointer, to pass to C.
    pub const fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<T> Eq for Handle<T> {}

impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle({:p})", self.ptr.as_ptr())
    }
}

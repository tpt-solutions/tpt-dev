//! Typestate sessions over a stateful C-style API.
//!
//! A session moves through states: `Closed`, `Open` and `Streaming`. Each
//! operation takes the session by value and returns it in its new state, so an
//! operation that is not allowed in the current state does not compile. A
//! failed operation returns the session unchanged, with the error code.
//! Dropping a session releases its resource unless it is already closed.
//!
//! # Ownership and borrow rules
//!
//! 1. The session owns the library object. It is destroyed exactly once, when
//!    the session is dropped, in whatever state it is in.
//! 2. Transitions take the session by value. A handle never outlives the
//!    session that produced it, because [`Session::with`] and
//!    [`Session::with_view`] borrow the session for the call.
//! 3. Buffers passed to the library are borrowed for one call, through
//!    [`Session::with`]. Nothing about them is kept after the call returns.
//! 4. A zero-copy [`View`] of library memory exists only inside the closure
//!    given to [`Session::with_view`]. The closure takes the view for any
//!    lifetime (`for<'v>`), so the view cannot be stored or returned. The
//!    session is borrowed mutably for the whole call, so no library call can
//!    run while the view is alive.
//! 5. [`Backend::view`] returns a slice tied to the backend borrow. The
//!    implementer guarantees that the slice stays valid and unchanged for that
//!    borrow.

use core::marker::PhantomData;

use crate::Code;

/// The operations a C-style library provides. Implement this for your library.
///
/// The handle type is whatever the library hands back, usually a [`crate::Handle`].
pub trait Backend {
    /// The library's handle for an open session.
    type Handle: Copy;

    /// Creates a session object, not yet open.
    fn create(&mut self) -> Result<Self::Handle, Code>;
    /// Opens the session.
    fn open(&mut self, h: Self::Handle) -> Result<(), Code>;
    /// Starts streaming on an open session.
    fn start(&mut self, h: Self::Handle) -> Result<(), Code>;
    /// Stops streaming.
    fn stop(&mut self, h: Self::Handle) -> Result<(), Code>;
    /// Closes the session.
    fn close(&mut self, h: Self::Handle) -> Result<(), Code>;
    /// Frees the session object. Called once, from `Drop`.
    fn destroy(&mut self, h: Self::Handle);
    /// Borrows the library's bytes for `h` without copying, if the library
    /// exposes them. The default exposes none.
    ///
    /// The returned slice must stay valid and unchanged for the whole borrow
    /// of `self`. The session guarantees that no other library call runs while
    /// the borrow lives.
    fn view(&mut self, _h: Self::Handle) -> Option<&[u8]> {
        None
    }
}

/// A read-only, zero-copy view of library memory.
///
/// A view can only be made inside the closure passed to
/// [`Session::with_view`]. It cannot be stored, returned, or used after that
/// call, and it cannot coexist with a library call on the same session. The
/// compile-fail cases in `tests/ui/view_*.rs` check this.
pub struct View<'v> {
    bytes: &'v [u8],
}

impl View<'_> {
    /// The viewed bytes. They are valid only inside the view's closure.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ffi::{Backend, Code, Session};
    ///
    /// struct Bytes(Vec<u8>);
    /// impl Backend for Bytes {
    ///     type Handle = u32;
    ///     fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    ///     fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn destroy(&mut self, _: u32) {}
    ///     fn view(&mut self, _: u32) -> Option<&[u8]> { Some(&self.0) }
    /// }
    ///
    /// let mut backend = Bytes(vec![1, 2, 3]);
    /// let mut streaming = Session::new(&mut backend)
    ///     .create()
    ///     .unwrap()
    ///     .open()
    ///     .unwrap()
    ///     .start()
    ///     .unwrap();
    /// let len = streaming.with_view(|view| view.as_slice().len()).unwrap();
    /// assert_eq!(len, 3);
    /// ```
    pub fn as_slice(&self) -> &[u8] {
        self.bytes
    }
}

impl core::ops::Deref for View<'_> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.bytes
    }
}

impl core::fmt::Debug for View<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("View")
            .field("len", &self.bytes.len())
            .finish()
    }
}

/// State: created, not open.
#[derive(Debug)]
pub struct Closed;
/// State: open, not streaming.
#[derive(Debug)]
pub struct Open;
/// State: streaming.
#[derive(Debug)]
pub struct Streaming;

/// An operation that failed. It returns the session in the state it had
/// before the call, and the code the library reported.
#[derive(Debug)]
pub struct Failed<S> {
    /// The session, unchanged.
    pub session: S,
    /// The library's error code.
    pub code: Code,
}

/// The resource a session owns: the backend and the library object. Dropping
/// it frees the object, so moving a session between states keeps the object
/// alive until the last state is dropped.
struct Inner<'b, B: Backend> {
    backend: &'b mut B,
    handle: Option<B::Handle>,
}

impl<B: Backend> Drop for Inner<'_, B> {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            self.backend.destroy(h);
        }
    }
}

/// A session over a backend `B`, in state `S`.
///
/// ```
/// use tpt_dev_ffi::{Backend, Code, Session};
///
/// struct Fake { log: Vec<&'static str> }
/// impl Backend for Fake {
///     type Handle = u32;
///     fn create(&mut self) -> Result<u32, Code> { Ok(1) }
///     fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
///     fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
///     fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
///     fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
///     fn destroy(&mut self, _: u32) { self.log.push("destroyed"); }
/// }
///
/// let mut fake = Fake { log: Vec::new() };
/// let session = Session::new(&mut fake).create().unwrap();
/// let streaming = session.open().unwrap().start().unwrap();
/// let open = streaming.stop().unwrap();
/// drop(open); // destroys the session
/// assert_eq!(fake.log, ["destroyed"]);
/// ```
pub struct Session<'b, B: Backend, S> {
    inner: Inner<'b, B>,
    _state: PhantomData<S>,
}

impl<'b, B: Backend, S> Session<'b, B, S> {
    fn into_state<T>(self) -> Session<'b, B, T> {
        Session {
            inner: self.inner,
            _state: PhantomData,
        }
    }

    /// Runs `f` against the library, and changes state if it succeeds.
    fn transition<T>(
        self,
        f: impl FnOnce(&mut B, B::Handle) -> Result<(), Code>,
    ) -> Result<Session<'b, B, T>, Failed<Self>> {
        let Some(h) = self.inner.handle else {
            return Err(Failed {
                session: self,
                code: Code(-1),
            });
        };
        let result = f(self.inner.backend, h);
        match result {
            Ok(()) => Ok(self.into_state()),
            Err(code) => Err(Failed {
                session: self,
                code,
            }),
        }
    }

    /// Runs a library call that does not change state, such as a read. The
    /// call borrows the session, so the state cannot change while it runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ffi::{Backend, Code, Session};
    ///
    /// struct Lib;
    /// impl Backend for Lib {
    ///     type Handle = u32;
    ///     fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    ///     fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn destroy(&mut self, _: u32) {}
    /// }
    ///
    /// let mut lib = Lib;
    /// let mut session = Session::new(&mut lib).create().unwrap().open().unwrap();
    /// assert_eq!(session.with(|_, handle| handle), Some(1));
    /// ```
    pub fn with<R>(&mut self, f: impl FnOnce(&mut B, B::Handle) -> R) -> Option<R> {
        let h = self.inner.handle?;
        Some(f(self.inner.backend, h))
    }
}

impl<'b, B: Backend> Session<'b, B, Closed> {
    /// A session that has no library object yet.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ffi::{Backend, Code, Session};
    ///
    /// struct Lib;
    /// impl Backend for Lib {
    ///     type Handle = u32;
    ///     fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    ///     fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn destroy(&mut self, _: u32) {}
    /// }
    ///
    /// let mut lib = Lib;
    /// let session = Session::new(&mut lib);
    /// assert_eq!(format!("{session:?}"), "Session { open: false }");
    /// ```
    pub fn new(backend: &'b mut B) -> Self {
        Self {
            inner: Inner {
                backend,
                handle: None,
            },
            _state: PhantomData,
        }
    }

    /// Creates the library object. The session stays `Closed`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ffi::{Backend, Code, Session};
    ///
    /// struct Lib;
    /// impl Backend for Lib {
    ///     type Handle = u32;
    ///     fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    ///     fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn destroy(&mut self, _: u32) {}
    /// }
    ///
    /// let mut lib = Lib;
    /// let session = Session::new(&mut lib).create().unwrap();
    /// assert_eq!(format!("{session:?}"), "Session { open: true }");
    /// ```
    pub fn create(mut self) -> Result<Self, Failed<Self>> {
        match self.inner.backend.create() {
            Ok(h) => {
                self.inner.handle = Some(h);
                Ok(self)
            }
            Err(code) => Err(Failed {
                session: self,
                code,
            }),
        }
    }

    /// Opens the session.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ffi::{Backend, Code, Session};
    ///
    /// struct Lib;
    /// impl Backend for Lib {
    ///     type Handle = u32;
    ///     fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    ///     fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn destroy(&mut self, _: u32) {}
    /// }
    ///
    /// let mut lib = Lib;
    /// let session = Session::new(&mut lib).create().unwrap().open().unwrap();
    /// assert_eq!(format!("{session:?}"), "Session { open: true }");
    /// ```
    pub fn open(self) -> Result<Session<'b, B, Open>, Failed<Self>> {
        self.transition(|b, h| b.open(h))
    }
}

impl<'b, B: Backend> Session<'b, B, Open> {
    /// Starts streaming.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ffi::{Backend, Code, Session};
    ///
    /// struct Lib;
    /// impl Backend for Lib {
    ///     type Handle = u32;
    ///     fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    ///     fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn destroy(&mut self, _: u32) {}
    /// }
    ///
    /// let mut lib = Lib;
    /// let streaming = Session::new(&mut lib)
    ///     .create()
    ///     .unwrap()
    ///     .open()
    ///     .unwrap()
    ///     .start()
    ///     .unwrap();
    /// drop(streaming);
    /// ```
    pub fn start(self) -> Result<Session<'b, B, Streaming>, Failed<Self>> {
        self.transition(|b, h| b.start(h))
    }

    /// Closes the session. The library object is freed when the returned session is dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ffi::{Backend, Code, Session};
    ///
    /// struct Lib;
    /// impl Backend for Lib {
    ///     type Handle = u32;
    ///     fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    ///     fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn destroy(&mut self, _: u32) {}
    /// }
    ///
    /// let mut lib = Lib;
    /// let closed = Session::new(&mut lib)
    ///     .create()
    ///     .unwrap()
    ///     .open()
    ///     .unwrap()
    ///     .close()
    ///     .unwrap();
    /// drop(closed);
    /// ```
    pub fn close(self) -> Result<Session<'b, B, Closed>, Failed<Self>> {
        self.transition(|b, h| b.close(h))
    }
}

impl<'b, B: Backend> Session<'b, B, Streaming> {
    /// Stops streaming, back to `Open`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ffi::{Backend, Code, Session};
    ///
    /// struct Lib;
    /// impl Backend for Lib {
    ///     type Handle = u32;
    ///     fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    ///     fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn destroy(&mut self, _: u32) {}
    /// }
    ///
    /// let mut lib = Lib;
    /// let open = Session::new(&mut lib)
    ///     .create()
    ///     .unwrap()
    ///     .open()
    ///     .unwrap()
    ///     .start()
    ///     .unwrap()
    ///     .stop()
    ///     .unwrap();
    /// drop(open);
    /// ```
    pub fn stop(self) -> Result<Session<'b, B, Open>, Failed<Self>> {
        self.transition(|b, h| b.stop(h))
    }

    /// Gives `f` a zero-copy view of the library's bytes, if the backend exposes
    /// any. The view borrows the session for the call, and `f` must not keep it.
    /// Returns `None` when there is no object or no view.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ffi::{Backend, Code, Session};
    ///
    /// struct Bytes(Vec<u8>);
    /// impl Backend for Bytes {
    ///     type Handle = u32;
    ///     fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    ///     fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    ///     fn destroy(&mut self, _: u32) {}
    ///     fn view(&mut self, _: u32) -> Option<&[u8]> { Some(&self.0) }
    /// }
    ///
    /// let mut backend = Bytes(vec![7, 8]);
    /// let mut streaming = Session::new(&mut backend)
    ///     .create()
    ///     .unwrap()
    ///     .open()
    ///     .unwrap()
    ///     .start()
    ///     .unwrap();
    /// let first = streaming.with_view(|view| view[0]).unwrap();
    /// assert_eq!(first, 7);
    /// ```
    pub fn with_view<R>(&mut self, f: impl for<'v> FnOnce(View<'v>) -> R) -> Option<R> {
        let h = self.inner.handle?;
        let bytes = self.inner.backend.view(h)?;
        Some(f(View { bytes }))
    }
}

impl<B: Backend, S> core::fmt::Debug for Session<'_, B, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Session")
            .field("open", &self.inner.handle.is_some())
            .finish()
    }
}

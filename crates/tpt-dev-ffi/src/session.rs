//! Typestate sessions over a stateful C-style API.
//!
//! A session moves through states: `Closed`, `Open` and `Streaming`. Each
//! operation takes the session by value and returns it in its new state, so an
//! operation that is not allowed in the current state does not compile. A
//! failed operation returns the session unchanged, with the error code.
//! Dropping a session releases its resource unless it is already closed.

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
    pub fn with<R>(&mut self, f: impl FnOnce(&mut B, B::Handle) -> R) -> Option<R> {
        let h = self.inner.handle?;
        Some(f(self.inner.backend, h))
    }
}

impl<'b, B: Backend> Session<'b, B, Closed> {
    /// A session that has no library object yet.
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
    pub fn open(self) -> Result<Session<'b, B, Open>, Failed<Self>> {
        self.transition(|b, h| b.open(h))
    }
}

impl<'b, B: Backend> Session<'b, B, Open> {
    /// Starts streaming.
    pub fn start(self) -> Result<Session<'b, B, Streaming>, Failed<Self>> {
        self.transition(|b, h| b.start(h))
    }

    /// Closes the session. The library object is freed when the returned session is dropped.
    pub fn close(self) -> Result<Session<'b, B, Closed>, Failed<Self>> {
        self.transition(|b, h| b.close(h))
    }
}

impl<'b, B: Backend> Session<'b, B, Streaming> {
    /// Stops streaming, back to `Open`.
    pub fn stop(self) -> Result<Session<'b, B, Open>, Failed<Self>> {
        self.transition(|b, h| b.stop(h))
    }
}

impl<B: Backend, S> core::fmt::Debug for Session<'_, B, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Session")
            .field("open", &self.inner.handle.is_some())
            .finish()
    }
}

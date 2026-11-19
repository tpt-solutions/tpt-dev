//! A stateful C-style library, written in Rust with `extern "C"` functions, wrapped
//! by a typestate session. Each state transition is checked by the compiler, and
//! a failed call hands the session back unchanged.

// The test defines the C-style library itself, so it needs unsafe code. Every call
// passes a handle from `counter_create` that is live until `Drop` destroys it. That
// is the one contract, so per-call SAFETY comments would only repeat it.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    unsafe_code,
    clippy::undocumented_unsafe_blocks
)]

use tpt_dev_ffi::{Backend, Code, Handle, Session};

const CREATED: u8 = 0;
const OPEN: u8 = 1;
const STREAMING: u8 = 2;
const CLOSED: u8 = 3;

/// The library's object. Its state is checked by every call.
#[repr(C)]
pub struct Counter {
    state: u8,
    ticks: u32,
}

#[unsafe(no_mangle)]
extern "C" fn counter_create() -> *mut Counter {
    Box::into_raw(Box::new(Counter {
        state: CREATED,
        ticks: 0,
    }))
}

/// # Safety
/// `c` must come from `counter_create` and not have been destroyed.
#[unsafe(no_mangle)]
unsafe extern "C" fn counter_open(c: *mut Counter) -> i32 {
    // SAFETY: the caller passes a live pointer from `counter_create`.
    let c = unsafe { &mut *c };
    if c.state == CREATED {
        c.state = OPEN;
        0
    } else {
        -2
    }
}

/// # Safety
/// As for `counter_open`.
#[unsafe(no_mangle)]
unsafe extern "C" fn counter_start(c: *mut Counter) -> i32 {
    // SAFETY: as in `counter_open`.
    let c = unsafe { &mut *c };
    if c.state == OPEN {
        c.state = STREAMING;
        0
    } else {
        -2
    }
}

/// # Safety
/// As for `counter_open`.
#[unsafe(no_mangle)]
unsafe extern "C" fn counter_stop(c: *mut Counter) -> i32 {
    // SAFETY: as in `counter_open`.
    let c = unsafe { &mut *c };
    if c.state == STREAMING {
        c.state = OPEN;
        0
    } else {
        -2
    }
}

/// # Safety
/// As for `counter_open`.
#[unsafe(no_mangle)]
unsafe extern "C" fn counter_close(c: *mut Counter) -> i32 {
    // SAFETY: as in `counter_open`.
    let c = unsafe { &mut *c };
    if c.state == OPEN {
        c.state = CLOSED;
        0
    } else {
        -2
    }
}

/// # Safety
/// As for `counter_open`. Returns the tick count while streaming.
#[unsafe(no_mangle)]
unsafe extern "C" fn counter_read(c: *const Counter, out: *mut u32) -> i32 {
    // SAFETY: as in `counter_open`; `out` is a writable u32 supplied by the caller.
    let c = unsafe { &*c };
    if c.state != STREAMING {
        return -2;
    }
    unsafe { *out = c.ticks };
    0
}

/// # Safety
/// As for `counter_open`. Advances the tick count while streaming.
#[unsafe(no_mangle)]
unsafe extern "C" fn counter_tick(c: *mut Counter) -> i32 {
    // SAFETY: as in `counter_open`.
    let c = unsafe { &mut *c };
    if c.state != STREAMING {
        return -2;
    }
    c.ticks += 1;
    0
}

/// # Safety
/// `c` must come from `counter_create` and not be used again.
#[unsafe(no_mangle)]
unsafe extern "C" fn counter_destroy(c: *mut Counter) {
    // SAFETY: the caller hands over the pointer from `counter_create`, once.
    drop(unsafe { Box::from_raw(c) });
}

/// Calls the library through its C-style functions.
#[derive(Default)]
struct CBackend {
    destroyed: usize,
}

impl Backend for CBackend {
    type Handle = Handle<Counter>;

    fn create(&mut self) -> Result<Handle<Counter>, Code> {
        Handle::new(counter_create()).ok_or(Code(-1))
    }

    fn open(&mut self, h: Handle<Counter>) -> Result<(), Code> {
        check(unsafe { counter_open(h.as_ptr()) })
    }

    fn start(&mut self, h: Handle<Counter>) -> Result<(), Code> {
        check(unsafe { counter_start(h.as_ptr()) })
    }

    fn stop(&mut self, h: Handle<Counter>) -> Result<(), Code> {
        check(unsafe { counter_stop(h.as_ptr()) })
    }

    fn close(&mut self, h: Handle<Counter>) -> Result<(), Code> {
        check(unsafe { counter_close(h.as_ptr()) })
    }

    fn destroy(&mut self, h: Handle<Counter>) {
        self.destroyed += 1;
        unsafe { counter_destroy(h.as_ptr()) };
    }
}

fn check(code: i32) -> Result<(), Code> {
    if code == 0 { Ok(()) } else { Err(Code(code)) }
}

#[test]
fn full_lifecycle_through_the_typestates() {
    let mut backend = CBackend::default();
    let session = Session::new(&mut backend).create().unwrap();
    let streaming = session.open().unwrap().start().unwrap();
    let open = streaming.stop().unwrap();
    let closed = open.close().unwrap();
    drop(closed);
    assert_eq!(backend.destroyed, 1);
}

#[test]
fn reads_only_work_while_streaming() {
    let mut backend = CBackend::default();
    let session = Session::new(&mut backend).create().unwrap();
    let mut streaming = session.open().unwrap().start().unwrap();
    let mut value = 0u32;
    let code = streaming
        .with(|_, h| {
            unsafe { counter_tick(h.as_ptr()) };
            unsafe { counter_read(h.as_ptr(), &mut value) }
        })
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(value, 1);
}

/// A backend whose `start` fails with a code, as a busy library might.
struct Busy {
    inner: CBackend,
}

impl Backend for Busy {
    type Handle = Handle<Counter>;

    fn create(&mut self) -> Result<Handle<Counter>, Code> {
        self.inner.create()
    }

    fn open(&mut self, h: Handle<Counter>) -> Result<(), Code> {
        self.inner.open(h)
    }

    fn start(&mut self, _h: Handle<Counter>) -> Result<(), Code> {
        Err(Code(7))
    }

    fn stop(&mut self, h: Handle<Counter>) -> Result<(), Code> {
        self.inner.stop(h)
    }

    fn close(&mut self, h: Handle<Counter>) -> Result<(), Code> {
        self.inner.close(h)
    }

    fn destroy(&mut self, h: Handle<Counter>) {
        self.inner.destroy(h)
    }
}

#[test]
fn a_failing_start_hands_back_the_open_session() {
    let mut backend = Busy {
        inner: CBackend::default(),
    };
    {
        let open = Session::new(&mut backend).create().unwrap().open().unwrap();
        let failed = open.start().unwrap_err();
        assert_eq!(failed.code, Code(7));
        // The session is still open, so it can be closed normally.
        let closed = failed.session.close().unwrap();
        drop(closed);
    }
    assert_eq!(backend.inner.destroyed, 1);
}

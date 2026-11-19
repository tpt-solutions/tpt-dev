//! A stateful C-style library, written in Rust with `extern "C"` functions, wrapped
//! by a typestate session. Each state transition is checked by the compiler, and
//! a failed call hands the session back unchanged.

// The test defines the C-style library itself, so it needs unsafe code. Every call
// passes a handle from `counter_create` that is live until `Drop` destroys it. That
// is the one contract, so per-call SAFETY comments would only repeat it.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    missing_docs,
    unsafe_code,
    clippy::undocumented_unsafe_blocks
)]

use tpt_dev_automata::{Machine, StateId};
use tpt_dev_ffi::{Backend, Closed, Code, Handle, Open, Session, Streaming};

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
pub extern "C" fn counter_create() -> *mut Counter {
    Box::into_raw(Box::new(Counter {
        state: CREATED,
        ticks: 0,
    }))
}

/// # Safety
/// `c` must come from `counter_create` and not have been destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_open(c: *mut Counter) -> i32 {
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
pub unsafe extern "C" fn counter_start(c: *mut Counter) -> i32 {
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
pub unsafe extern "C" fn counter_stop(c: *mut Counter) -> i32 {
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
pub unsafe extern "C" fn counter_close(c: *mut Counter) -> i32 {
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
pub unsafe extern "C" fn counter_read(c: *const Counter, out: *mut u32) -> i32 {
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
pub unsafe extern "C" fn counter_tick(c: *mut Counter) -> i32 {
    // SAFETY: as in `counter_open`.
    let c = unsafe { &mut *c };
    if c.state != STREAMING {
        return -2;
    }
    c.ticks += 1;
    0
}

/// Writes the address and length of the tick count's bytes, while streaming.
/// # Safety
/// As for `counter_open`. The bytes stay valid until the counter is next mutated or destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_ticks_bytes(
    c: *const Counter,
    out: *mut *const u8,
    len: *mut usize,
) -> i32 {
    // SAFETY: as in `counter_open`; `out` and `len` are writable, supplied by the caller.
    let c = unsafe { &*c };
    if c.state != STREAMING {
        return -2;
    }
    unsafe {
        *out = core::ptr::addr_of!(c.ticks).cast::<u8>();
        *len = core::mem::size_of::<u32>();
    }
    0
}

/// # Safety
/// `c` must come from `counter_create` and not be used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_destroy(c: *mut Counter) {
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

    fn view(&mut self, h: Handle<Counter>) -> Option<&[u8]> {
        let mut ptr: *const u8 = core::ptr::null();
        let mut len = 0usize;
        if unsafe { counter_ticks_bytes(h.as_ptr(), &mut ptr, &mut len) } != 0 {
            return None;
        }
        // SAFETY: `counter_ticks_bytes` returns the tick field of the live counter,
        // which stays valid and unchanged while this backend is borrowed, because
        // the session makes no library call during that borrow.
        Some(unsafe { core::slice::from_raw_parts(ptr, len) })
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

#[test]
fn zero_copy_view_reads_the_library_memory_in_place() {
    let mut backend = CBackend::default();
    let mut streaming = Session::new(&mut backend)
        .create()
        .unwrap()
        .open()
        .unwrap()
        .start()
        .unwrap();
    streaming
        .with(|_, h| unsafe { counter_tick(h.as_ptr()) })
        .unwrap();
    streaming
        .with(|_, h| unsafe { counter_tick(h.as_ptr()) })
        .unwrap();
    let mut copied = [0u8; 4];
    let len = streaming
        .with_view(|view| {
            copied.copy_from_slice(&view);
            view.len()
        })
        .unwrap();
    assert_eq!(len, 4);
    assert_eq!(u32::from_ne_bytes(copied), 2);
}

#[test]
fn with_view_is_available_on_every_call_while_streaming() {
    let mut backend = CBackend::default();
    let mut streaming = Session::new(&mut backend)
        .create()
        .unwrap()
        .open()
        .unwrap()
        .start()
        .unwrap();
    // The view covers the four tick bytes, on every call while streaming.
    assert_eq!(streaming.with_view(|v| v.is_empty()), Some(false));
    assert_eq!(streaming.with_view(|v| v.len()), Some(4));
    let open = streaming.stop().unwrap();
    drop(open);
}

/// The C header for the library's exported functions, generated from this file.
#[test]
fn generated_header_declares_the_exported_functions() {
    let options = tpt_dev_headers::Options {
        guard: String::from("COUNTER_H"),
    };
    let header = tpt_dev_headers::generate(include_str!("c_library.rs"), &options).unwrap();
    for prototype in [
        "Counter *counter_create(void);",
        "int32_t counter_open(Counter *c);",
        "int32_t counter_start(Counter *c);",
        "int32_t counter_stop(Counter *c);",
        "int32_t counter_close(Counter *c);",
        "int32_t counter_read(const Counter *c, uint32_t *out);",
        "int32_t counter_tick(Counter *c);",
        "void counter_destroy(Counter *c);",
    ] {
        assert!(
            header.contains(prototype),
            "missing `{prototype}` in:
{header}"
        );
    }
    assert!(header.contains("#ifndef COUNTER_H"));
    assert!(header.contains("typedef struct Counter {"));
}

/// The session's typestates as a `tpt_dev_automata` machine.
fn session_machine() -> (Machine, [StateId; 3]) {
    let mut m = Machine::new();
    let closed = m.state("Closed");
    let open = m.state("Open");
    let streaming = m.state("Streaming");
    m.transition(closed, open, "open", None).unwrap();
    m.transition(open, streaming, "start", None).unwrap();
    m.transition(streaming, open, "stop", None).unwrap();
    m.transition(open, closed, "close", None).unwrap();
    (m, [closed, open, streaming])
}

fn has_transition(m: &Machine, from: StateId, event: &str, to: StateId) -> bool {
    m.transitions()
        .iter()
        .any(|t| t.from == from && t.to == to && t.event == event)
}

#[test]
fn session_machine_has_no_deadlocks_and_every_state_is_reachable() {
    let (mut machine, [closed, open, streaming]) = session_machine();
    let analysis = machine.analyze(closed).unwrap();
    assert!(analysis.is_deadlock_free(), "{:?}", analysis.deadlocks);
    assert_eq!(analysis.reachable, [closed, open, streaming]);
    assert!(analysis.unreachable.is_empty());
}

#[test]
fn machine_detects_a_deadlock_when_a_transition_is_missing() {
    // Without `stop`, a streaming session has no way out. The analysis must say so.
    let mut m = Machine::new();
    let closed = m.state("Closed");
    let open = m.state("Open");
    let streaming = m.state("Streaming");
    m.transition(closed, open, "open", None).unwrap();
    m.transition(open, streaming, "start", None).unwrap();
    let analysis = m.analyze(closed).unwrap();
    assert_eq!(analysis.deadlocks.len(), 1);
    assert_eq!(analysis.deadlocks[0].state, streaming);
    assert_eq!(analysis.deadlocks[0].trace, ["open", "start"]);
}

#[test]
fn typestate_transitions_are_the_machine_transitions() {
    let (machine, [closed, open, streaming]) = session_machine();
    let mut backend = CBackend::default();
    let mut taken = Vec::new();

    let created = Session::new(&mut backend).create().unwrap();
    taken.push((closed, "open", open));
    let opened: Session<'_, CBackend, Open> = created.open().unwrap();
    taken.push((open, "start", streaming));
    let streaming_session: Session<'_, CBackend, Streaming> = opened.start().unwrap();
    taken.push((streaming, "stop", open));
    let reopened: Session<'_, CBackend, Open> = streaming_session.stop().unwrap();
    taken.push((open, "close", closed));
    let closed_session: Session<'_, CBackend, Closed> = reopened.close().unwrap();
    drop(closed_session);

    for &(from, event, to) in &taken {
        assert!(
            has_transition(&machine, from, event, to),
            "`{event}` from {from:?} to {to:?} is not a machine transition"
        );
    }
    // Every machine transition is exercised by the typestate code.
    assert_eq!(taken.len(), machine.transitions().len());
    assert_eq!(backend.destroyed, 1);
}

//! The typestate `Session` agrees with the guard generated from the session machine.
//!
//! The guard comes from `examples/generated/session_guard.rs`, which
//! `examples/machine_codegen.rs` writes. For every event sequence up to length
//! five, this test runs the same events through the guard and through the real
//! typestate API. An event is legal in the guard exactly when the typestate
//! has a method for it, and the two must reach the same state.

#![allow(clippy::unwrap_used, clippy::panic)]

#[path = "../examples/generated/session_guard.rs"]
#[allow(dead_code, unreachable_pub)]
mod guard;

use tpt_dev_ffi::{Backend, Closed, Code, Failed, Open, Session, Streaming};

use guard::{Event, State};

/// A backend that accepts every call and keeps no state of its own.
struct Fake;

impl Backend for Fake {
    type Handle = u32;
    fn create(&mut self) -> Result<u32, Code> {
        Ok(1)
    }
    fn open(&mut self, _h: u32) -> Result<(), Code> {
        Ok(())
    }
    fn start(&mut self, _h: u32) -> Result<(), Code> {
        Ok(())
    }
    fn stop(&mut self, _h: u32) -> Result<(), Code> {
        Ok(())
    }
    fn close(&mut self, _h: u32) -> Result<(), Code> {
        Ok(())
    }
    fn destroy(&mut self, _h: u32) {}
}

/// The typestate session in whichever state it is in.
enum Any<'b> {
    Closed(Session<'b, Fake, Closed>),
    Open(Session<'b, Fake, Open>),
    Streaming(Session<'b, Fake, Streaming>),
}

impl Any<'_> {
    fn state(&self) -> State {
        match self {
            Any::Closed(_) => State::Closed,
            Any::Open(_) => State::Open,
            Any::Streaming(_) => State::Streaming,
        }
    }
}

/// Applies `event` through the typestate API. `Err` means the typestate has no
/// method for this event in this state (the call would not compile), and hands
/// the session back unchanged.
fn apply(session: Any<'_>, event: Event) -> Result<Any<'_>, Any<'_>> {
    match (session, event) {
        (Any::Closed(s), Event::Open) => match s.open() {
            Ok(n) => Ok(Any::Open(n)),
            Err(Failed { session, .. }) => panic!("the fake backend never fails, got {session:?}"),
        },
        (Any::Open(s), Event::Start) => match s.start() {
            Ok(n) => Ok(Any::Streaming(n)),
            Err(Failed { session, .. }) => panic!("the fake backend never fails, got {session:?}"),
        },
        (Any::Open(s), Event::Close) => match s.close() {
            Ok(n) => Ok(Any::Closed(n)),
            Err(Failed { session, .. }) => panic!("the fake backend never fails, got {session:?}"),
        },
        (Any::Streaming(s), Event::Stop) => match s.stop() {
            Ok(n) => Ok(Any::Open(n)),
            Err(Failed { session, .. }) => panic!("the fake backend never fails, got {session:?}"),
        },
        (other, _) => Err(other),
    }
}

const EVENTS: [Event; 4] = [Event::Open, Event::Start, Event::Stop, Event::Close];

/// Runs one event sequence through both models and checks they agree at every step.
fn check_sequence(seq: &[Event]) {
    let mut fake = Fake;
    let created = Session::new(&mut fake).create().unwrap();
    let mut session = Any::Closed(created);
    let mut model = State::Closed;

    for &event in seq {
        let expected = guard::next(model, event);
        match apply(session, event) {
            Ok(next) => {
                let want = expected.unwrap_or_else(|| {
                    panic!("typestate accepts {event:?} in {model:?}, guard does not")
                });
                assert_eq!(next.state(), want, "after {event:?} from {model:?}");
                model = want;
                session = next;
            }
            Err(same) => {
                assert!(
                    expected.is_none(),
                    "guard allows {event:?} in {model:?}, typestate has no method"
                );
                assert_eq!(same.state(), model);
                session = same;
            }
        }
    }
}

/// Every sequence of length `0..=max`, over all events.
fn all_sequences(max: usize) -> Vec<Vec<Event>> {
    let mut out = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];
    for _ in 0..max {
        let mut next = Vec::new();
        for seq in &frontier {
            for &e in &EVENTS {
                let mut s = seq.clone();
                s.push(e);
                next.push(s);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

#[test]
fn guard_matches_typestate_on_every_sequence_up_to_length_five() {
    let seqs = all_sequences(5);
    assert_eq!(seqs.len(), 1 + 4 + 16 + 64 + 256 + 1024);
    for seq in &seqs {
        check_sequence(seq);
    }
}

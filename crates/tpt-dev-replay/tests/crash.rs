//! Replaying a concurrent crash bit-for-bit.
//!
//! Two producers push inputs into a shared cell, and a watcher panics if it
//! sees the value 7. Whether that happens depends on the schedule and the
//! inputs, so some seeds crash. A recording that crashes is replayed, and the
//! replay must crash with the same message, after the same events, leaving an
//! identical log.

#![allow(clippy::unwrap_used, clippy::panic)]

use core::cell::{Cell, RefCell};
use core::future::Future;
use core::pin::{Pin, pin};
use core::task::{Context, Poll};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::string::String;

use tpt_dev_async::Executor;
use tpt_dev_replay::{Log, Progress, Session, step};

const CAP: usize = 256;

/// Reads `left` inputs from its channel, storing each one in `last`.
struct Produce<'a> {
    id: u8,
    session: &'a RefCell<Session<CAP>>,
    last: &'a Cell<u64>,
    left: u32,
}

impl Future for Produce<'_> {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.left == 0 {
            return Poll::Ready(());
        }
        let id = self.id;
        let value = self
            .session
            .borrow_mut()
            .input(id)
            .expect("input divergence")
            % 8;
        self.last.set(value);
        self.left -= 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// Watches `last` for a bad value.
struct Watch<'a> {
    last: &'a Cell<u64>,
    polls: u32,
}

impl Future for Watch<'_> {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.last.get() == 7 {
            panic!("watcher saw the bad value 7 after {} polls", self.polls);
        }
        if self.polls == 6 {
            return Poll::Ready(());
        }
        self.polls += 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// Runs the scenario until it finishes, panics, or the driver stops.
fn run(session: &RefCell<Session<CAP>>) {
    let last = Cell::new(0);
    let mut exec = pin!(Executor::<3>::new());
    let p0 = pin!(Produce {
        id: 0,
        session,
        last: &last,
        left: 3
    });
    let p1 = pin!(Produce {
        id: 1,
        session,
        last: &last,
        left: 3
    });
    let w = pin!(Watch {
        last: &last,
        polls: 0
    });
    exec.as_mut().spawn(p0, 2).unwrap();
    exec.as_mut().spawn(p1, 2).unwrap();
    exec.as_mut().spawn(w, 1).unwrap();
    for _ in 0..200 {
        match step(session, exec.as_mut()) {
            Ok(Progress::Idle) => break,
            Ok(Progress::Polled { .. }) => {}
            Err(d) => panic!("divergence at {}: {}", d.at, d.expected),
        }
    }
}

fn message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        String::from(*s)
    } else {
        String::from("<non-string panic>")
    }
}

/// Outcome of one run: the crash message, if any, and the log it left behind.
struct Outcome {
    crash: Option<String>,
    log: Log<CAP>,
}

fn execute(session: Session<CAP>) -> Outcome {
    let cell = RefCell::new(session);
    let result = catch_unwind(AssertUnwindSafe(|| run(&cell)));
    let crash = result.err().map(|p| message(p.as_ref()));
    let log = *cell.borrow().log();
    Outcome { crash, log }
}

#[test]
fn a_crash_replays_bit_for_bit() {
    // Find a seed whose recording crashes.
    let mut recorded = None;
    for seed in 1..=300u64 {
        let mut s = Session::recording(seed);
        s.set_interrupt_permille(250);
        let out = execute(s);
        if out.crash.is_some() {
            recorded = Some((seed, out));
            break;
        }
    }
    let (seed, original) = recorded.expect("some seed should crash");
    let crash = original.crash.clone().unwrap();
    assert!(crash.contains("bad value 7"), "unexpected crash: {crash}");

    // Replay the log. It must crash the same way and leave the same log.
    let replayed = execute(Session::replaying(original.log));
    assert_eq!(
        replayed.crash.as_deref(),
        Some(crash.as_str()),
        "seed {seed}"
    );
    assert_eq!(replayed.log, original.log, "seed {seed}");
    assert!(!original.log.is_empty());
}

#[test]
fn a_changed_log_is_reported_as_divergence() {
    // Find a crashing recording, then change one input in its log.
    let mut found = None;
    for seed in 1..=300u64 {
        let mut s = Session::recording(seed);
        s.set_interrupt_permille(250);
        let out = execute(s);
        if out.crash.is_some() {
            found = Some(out.log);
            break;
        }
    }
    let log = found.expect("some seed should crash");
    let mut tampered: Log<CAP> = Log::new();
    let mut changed = false;
    for e in log.as_slice() {
        let e2 = match *e {
            tpt_dev_replay::Event::Input { channel, value } if !changed => {
                changed = true;
                tpt_dev_replay::Event::Input {
                    channel: channel ^ 1,
                    value,
                }
            }
            other => other,
        };
        tampered.push(e2).unwrap();
    }
    assert!(changed);
    let out = execute(Session::replaying(tampered));
    let msg = out.crash.expect("a tampered replay must fail");
    assert!(msg.contains("divergence"), "got: {msg}");
}

#[test]
fn recording_without_crash_runs_to_idle() {
    // With no interrupts and a benign seed, the scenario completes and goes idle.
    let mut s: Session<CAP> = Session::recording(12345);
    s.set_interrupt_permille(0);
    let cell = RefCell::new(s);
    let mut exec_steps = 0;
    let last = Cell::new(0);
    let mut exec = pin!(Executor::<1>::new());
    let w = pin!(Watch {
        last: &last,
        polls: 0
    });
    exec.as_mut().spawn(w, 1).unwrap();
    loop {
        match step(&cell, exec.as_mut()).unwrap() {
            Progress::Idle => break,
            Progress::Polled { .. } => exec_steps += 1,
        }
    }
    // Six pending polls, then the ready poll.
    assert_eq!(exec_steps, 7);
}

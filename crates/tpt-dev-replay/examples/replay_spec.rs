//! Checking a replayed run against a state machine spec.
//!
//! A recorded run is a log of scheduling decisions, interrupts and inputs. The
//! same program can be replayed from the log, and the replay must produce the
//! same log. This example also checks the log against a protocol written as an
//! `automata` machine:
//!
//! - an interrupt may arrive at any time,
//! - a task may only read an input after the executor has polled it, and
//! - the run performs exactly six reads.
//!
//! The example records several runs, replays each one, and checks each log
//! against the spec. It then shows that a bad log is rejected both by the spec
//! and by the replay.
//!
//! Usage:
//!
//! ```text
//! cargo run -p tpt-dev-replay --example replay_spec
//! ```

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::panic,
    clippy::unwrap_used
)]

use core::cell::{Cell, RefCell};
use core::future::Future;
use core::pin::{Pin, pin};
use core::task::{Context, Poll};
use std::process;

use tpt_dev_async::Executor;
use tpt_dev_automata::{Machine, StateId};
use tpt_dev_replay::{Divergence, Event, Log, Progress, Session, step};

const CAP: usize = 256;
const READS: usize = 6;
const SEEDS: u64 = 20;

/// Reads `left` inputs from channel `id`.
struct Produce<'a> {
    id: u8,
    session: &'a RefCell<Session<CAP>>,
    /// Set when the replay asks for an input the log does not have.
    error: &'a Cell<Option<Divergence>>,
    left: u32,
}

impl Future for Produce<'_> {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.left == 0 {
            return Poll::Ready(());
        }
        let id = self.id;
        if let Err(d) = self.session.borrow_mut().input(id) {
            self.error.set(Some(d));
            return Poll::Ready(());
        }
        self.left -= 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// Two tasks, three reads each: six reads in total.
fn run(cell: &RefCell<Session<CAP>>) -> Result<(), Divergence> {
    let error = Cell::new(None);
    let mut exec = pin!(Executor::<2>::new());
    let p0 = pin!(Produce {
        id: 0,
        session: cell,
        error: &error,
        left: 3
    });
    let p1 = pin!(Produce {
        id: 1,
        session: cell,
        error: &error,
        left: 3
    });
    exec.as_mut().spawn(p0, 2).unwrap();
    exec.as_mut().spawn(p1, 2).unwrap();
    for _ in 0..200 {
        if let Some(d) = error.get() {
            return Err(d);
        }
        match step(cell, exec.as_mut()) {
            Ok(Progress::Idle) => break,
            Ok(Progress::Polled { .. }) => {}
            Err(d) => return Err(d),
        }
    }
    match error.get() {
        Some(d) => Err(d),
        None => Ok(()),
    }
}

/// The events of the spec, as a name for each log entry.
fn event_name(e: &Event) -> &'static str {
    match e {
        Event::Schedule { .. } => "poll",
        Event::Interrupt { .. } => "wake",
        Event::Input { .. } => "read",
    }
}

/// The protocol: wakes may come at any time, reads must follow a poll, and
/// there must be exactly `READS` reads. Returns the machine and its initial state.
fn spec() -> (Machine, StateId, Vec<StateId>) {
    let mut m = Machine::new();
    let init = m.state("init");
    // `counts[i]` is the state after i reads.
    let counts: Vec<StateId> = (0..=READS)
        .map(|i| m.state(&format!("reads_{i}")))
        .collect();
    m.mark_final(counts[READS]);

    // Before the first poll, only wakes are allowed.
    m.transition(init, init, "wake", None).unwrap();
    m.transition(init, counts[0], "poll", None).unwrap();
    for (i, &s) in counts.iter().enumerate() {
        m.transition(s, s, "poll", None).unwrap();
        m.transition(s, s, "wake", None).unwrap();
        if i < READS {
            m.transition(s, counts[i + 1], "read", None).unwrap();
        }
    }
    (m, init, counts)
}

/// Walks the spec over the events of a log. Returns the state reached, or the
/// index and name of the first event the spec does not allow.
fn accepts(m: &Machine, initial: StateId, log: &[Event]) -> Result<StateId, (usize, &'static str)> {
    let mut cur = initial;
    for (i, e) in log.iter().enumerate() {
        let name = event_name(e);
        let next = m
            .transitions()
            .iter()
            .find(|t| t.from == cur && t.event == name)
            .map(|t| t.to);
        match next {
            Some(to) => cur = to,
            None => return Err((i, name)),
        }
    }
    Ok(cur)
}

fn record(seed: u64) -> Result<Log<CAP>, String> {
    let mut s = Session::recording(seed);
    s.set_interrupt_permille(250);
    let cell = RefCell::new(s);
    run(&cell).map_err(|d| format!("recording diverged at {}: {}", d.at, d.expected))?;
    let log = *cell.borrow().log();
    Ok(log)
}

fn replay(log: Log<CAP>) -> Result<Log<CAP>, String> {
    let cell = RefCell::new(Session::replaying(log));
    run(&cell).map_err(|d| format!("replay diverged at {}: {}", d.at, d.expected))?;
    let out = *cell.borrow().log();
    Ok(out)
}

fn main() {
    let (mut machine, init, counts) = spec();
    let analysis = machine.analyze(init).expect("initial state exists");
    println!(
        "spec: {} states, deadlock-free: {}",
        counts.len() + 1,
        analysis.is_deadlock_free()
    );
    if !analysis.is_deadlock_free() {
        process::exit(1);
    }

    let mut failures = 0;
    let mut interrupts_before_poll = 0;
    for seed in 1..=SEEDS {
        let recorded = match record(seed) {
            Ok(l) => l,
            Err(msg) => {
                println!("seed {seed}: {msg}");
                failures += 1;
                continue;
            }
        };
        let replayed = match replay(recorded) {
            Ok(l) => l,
            Err(msg) => {
                println!("seed {seed}: {msg}");
                failures += 1;
                continue;
            }
        };
        if replayed.as_slice() != recorded.as_slice() {
            println!("seed {seed}: replay log differs from the recording");
            failures += 1;
            continue;
        }
        if let Some(Event::Interrupt { .. }) = recorded.as_slice().first() {
            interrupts_before_poll += 1;
        }
        match accepts(&machine, init, replayed.as_slice()) {
            Ok(state) if state == *counts.last().unwrap() => {}
            Ok(_) => {
                println!("seed {seed}: the run did not finish all {READS} reads");
                failures += 1;
            }
            Err((i, name)) => {
                println!("seed {seed}: spec rejects event {i} ({name})");
                failures += 1;
            }
        }
    }
    println!(
        "{SEEDS} recorded runs replayed bit-for-bit and checked against the spec; \
         {interrupts_before_poll} began with an interrupt; {failures} failures"
    );

    // A bad log: a read before any poll. The spec must reject it, and a replay of it must diverge.
    let mut bad = Log::<CAP>::new();
    bad.push(Event::Input {
        channel: 0,
        value: 3,
    })
    .unwrap();
    match accepts(&machine, init, bad.as_slice()) {
        Err((0, "read")) => println!("bad log: spec rejects a read before any poll"),
        other => {
            println!("bad log: spec should reject it, got {other:?}");
            failures += 1;
        }
    }
    match replay(bad) {
        Err(msg) => println!("bad log: replay also refuses it ({msg})"),
        Ok(_) => {
            println!("bad log: replay accepted it");
            failures += 1;
        }
    }

    if failures > 0 {
        process::exit(1);
    }
}

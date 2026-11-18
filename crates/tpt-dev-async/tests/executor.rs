//! Executor behaviour on the host: priorities, wake-ups from other threads,
//! capacity, timers, and wakers that outlive their task.

#![allow(clippy::unwrap_used, clippy::panic)]

use core::cell::Cell;
use core::future::Future;
use core::pin::{Pin, pin};
use core::task::{Context, Poll, Waker};
use std::sync::Mutex;
use std::thread;

use tpt_dev_async::{Delay, Executor, Full};
use tpt_dev_trace::ManualClock;

/// Records its id in `log` when first polled, and finishes.
struct Mark<'a> {
    id: u8,
    log: &'a Cell<u32>,
}

impl Future for Mark<'_> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        self.log.set(self.log.get() * 10 + u32::from(self.id));
        Poll::Ready(())
    }
}

#[test]
fn higher_priority_runs_first() {
    let log = Cell::new(0);
    let low = pin!(Mark { id: 1, log: &log });
    let high = pin!(Mark { id: 2, log: &log });
    let mut exec = pin!(Executor::<4>::new());
    exec.as_mut().spawn(low, 1).unwrap();
    exec.as_mut().spawn(high, 9).unwrap();
    assert_eq!(exec.as_mut().run_round(), 2);
    // 2 (priority 9) was polled before 1 (priority 1).
    assert_eq!(log.get(), 21);
    assert_eq!(exec.pending(), 0);
}

#[test]
fn spawn_reports_full() {
    let log = Cell::new(0);
    let a = pin!(Mark { id: 1, log: &log });
    let b = pin!(Mark { id: 2, log: &log });
    let mut exec = pin!(Executor::<1>::new());
    exec.as_mut().spawn(a, 0).unwrap();
    assert_eq!(exec.as_mut().spawn(b, 0), Err(Full));
}

/// Stores its waker, then waits until another thread sets `done`.
struct WaitFor<'a> {
    shared: &'a Mutex<(bool, Option<Waker>)>,
}

impl Future for WaitFor<'_> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut guard = self.shared.lock().unwrap();
        if guard.0 {
            return Poll::Ready(());
        }
        guard.1 = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[test]
fn wake_from_another_thread_is_seen() {
    let shared = Mutex::new((false, None::<Waker>));
    let task = pin!(WaitFor { shared: &shared });
    let mut exec = pin!(Executor::<1>::new());
    exec.as_mut().spawn(task, 0).unwrap();
    exec.as_mut().run_round();
    assert_eq!(exec.pending(), 1, "the task is waiting");

    thread::scope(|s| {
        s.spawn(|| {
            let waker = {
                let mut guard = shared.lock().unwrap();
                guard.0 = true;
                guard.1.take()
            };
            if let Some(w) = waker {
                w.wake();
            }
        });
    });
    assert_eq!(exec.as_mut().run_until_idle(4), 1);
    assert_eq!(exec.pending(), 0);
}

#[test]
fn delay_completes_after_clock_advances() {
    let clock = ManualClock::new(100);
    let delay = pin!(async {
        Delay::new(&clock, 5).await;
    });
    let mut exec = pin!(Executor::<1>::new());
    exec.as_mut().spawn(delay, 0).unwrap();
    // The delay keeps waking itself, so the executor keeps polling it.
    assert_eq!(exec.as_mut().run_until_idle(3), 3);
    assert_eq!(exec.pending(), 1);
    clock.advance(5);
    exec.as_mut().run_until_idle(2);
    assert_eq!(exec.pending(), 0);
}

#[test]
fn rounds_poll_at_most_capacity() {
    let log = Cell::new(0);
    let a = pin!(Mark { id: 1, log: &log });
    let b = pin!(Mark { id: 2, log: &log });
    let mut exec = pin!(Executor::<2>::new());
    exec.as_mut().spawn(a, 0).unwrap();
    exec.as_mut().spawn(b, 0).unwrap();
    exec.as_mut().run_round();
    let stats = exec.stats();
    assert_eq!(stats.rounds, 1);
    assert_eq!(stats.polls, 2);
    assert!(stats.max_polls_per_round <= 2);
}

#[test]
fn waker_kept_after_finish_is_harmless() {
    let shared = Mutex::new((false, None::<Waker>));
    let task = pin!(WaitFor { shared: &shared });
    let mut exec = pin!(Executor::<1>::new());
    exec.as_mut().spawn(task, 0).unwrap();
    exec.as_mut().run_round();
    // Finish the task, then wake through the stored waker after it is gone.
    shared.lock().unwrap().0 = true;
    let stored = shared.lock().unwrap().1.clone().unwrap();
    stored.wake_by_ref();
    exec.as_mut().run_round();
    assert_eq!(exec.pending(), 0);
    stored.wake_by_ref();
    stored.wake();
    // The finished slot's bit is ignored, so nothing is polled.
    assert_eq!(exec.as_mut().run_round(), 0);
}

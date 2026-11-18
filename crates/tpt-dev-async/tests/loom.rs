//! Model-checked wake-up protocol. Run with:
//! `RUSTFLAGS="--cfg loom" cargo test -p tpt-dev-async --release --test loom`
#![cfg(loom)]

use core::future::Future;
use core::pin::{Pin, pin};
use core::task::{Context, Poll, Waker};

use loom::sync::{Arc, Mutex};
use loom::thread;
use tpt_dev_async::Executor;

/// Checks `done` and stores its waker under one lock, so a wake cannot be lost.
struct WaitFor {
    shared: Arc<Mutex<(bool, Option<Waker>)>>,
}

impl Future for WaitFor {
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
fn wake_from_another_thread_is_never_lost() {
    loom::model(|| {
        let shared = Arc::new(Mutex::new((false, None::<Waker>)));
        let task = pin!(WaitFor {
            shared: shared.clone()
        });
        let mut exec = pin!(Executor::<1>::new());
        exec.as_mut().spawn(task, 0).unwrap();

        let s2 = shared.clone();
        let waker_thread = thread::spawn(move || {
            let waker = {
                let mut guard = s2.lock().unwrap();
                guard.0 = true;
                guard.1.take()
            };
            if let Some(w) = waker {
                w.wake();
            }
        });

        // Run concurrently with the waker thread, then wait for it.
        exec.as_mut().run_round();
        waker_thread.join().unwrap();
        exec.as_mut().run_until_idle(4);
        assert_eq!(exec.pending(), 0, "the task was never woken");
    });
}

#[test]
fn wake_bits_from_two_threads_both_arrive() {
    loom::model(|| {
        // Two tasks, each woken by its own thread. Both wake bits must be seen.
        let a = Arc::new(Mutex::new((false, None::<Waker>)));
        let b = Arc::new(Mutex::new((false, None::<Waker>)));
        let ta = pin!(WaitFor { shared: a.clone() });
        let tb = pin!(WaitFor { shared: b.clone() });
        let mut exec = pin!(Executor::<2>::new());
        exec.as_mut().spawn(ta, 0).unwrap();
        exec.as_mut().spawn(tb, 0).unwrap();

        let handles: Vec<_> = [a, b]
            .into_iter()
            .map(|s| {
                thread::spawn(move || {
                    let waker = {
                        let mut guard = s.lock().unwrap();
                        guard.0 = true;
                        guard.1.take()
                    };
                    if let Some(w) = waker {
                        w.wake();
                    }
                })
            })
            .collect();

        exec.as_mut().run_round();
        for h in handles {
            h.join().unwrap();
        }
        exec.as_mut().run_until_idle(4);
        assert_eq!(exec.pending(), 0);
    });
}

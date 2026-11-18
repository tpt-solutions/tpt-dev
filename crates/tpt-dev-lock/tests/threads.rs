//! Multi-threaded tests on the standard library: ordering, no lost or duplicated items.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use tpt_dev_lock::{L1, L2, Mutex, Spsc, Token, TripleBuffer};

#[test]
fn spsc_preserves_order_across_threads() {
    const COUNT: u64 = 200_000;
    let mut q: Spsc<u64, 64> = Spsc::new();
    let (mut tx, mut rx) = q.split();
    thread::scope(|s| {
        s.spawn(move || {
            for i in 0..COUNT {
                let mut v = i;
                while let Err(back) = tx.push(v) {
                    v = back;
                    thread::yield_now();
                }
            }
        });
        s.spawn(move || {
            let mut expected = 0;
            while expected < COUNT {
                match rx.pop() {
                    Some(v) => {
                        assert_eq!(v, expected);
                        expected += 1;
                    }
                    None => thread::yield_now(),
                }
            }
            assert_eq!(rx.pop(), None);
        });
    });
}

#[test]
fn triple_buffer_reader_sees_monotonic_values() {
    const LAST: u32 = 100_000;
    let mut buf = TripleBuffer::new(0u32, 0, 0);
    let (mut w, mut r) = buf.split();
    thread::scope(|s| {
        s.spawn(move || {
            for i in 1..=LAST {
                w.write(i);
            }
        });
        s.spawn(move || {
            let mut prev = 0;
            loop {
                let seen = r.with_latest(|v| *v);
                assert!(seen >= prev, "value went backwards: {seen} < {prev}");
                prev = seen;
                if seen == LAST {
                    break;
                }
                thread::yield_now();
            }
        });
    });
}

#[test]
fn triple_buffer_sees_latest_after_writes() {
    let mut buf = TripleBuffer::new(0u32, 0, 0);
    let (mut w, mut r) = buf.split();
    w.write(1);
    w.write(2);
    w.write(3);
    r.with_latest(|v| assert_eq!(*v, 3));
    // Nothing new was written, so the same value is seen again.
    r.with_latest(|v| assert_eq!(*v, 3));
}

static LIVE: AtomicUsize = AtomicUsize::new(0);

struct Counted;

impl Counted {
    fn new() -> Self {
        LIVE.fetch_add(1, Ordering::SeqCst);
        Counted
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

#[test]
fn spsc_drops_unpopped_items() {
    LIVE.store(0, Ordering::SeqCst);
    {
        let mut q: Spsc<Counted, 8> = Spsc::new();
        let (mut tx, mut rx) = q.split();
        for _ in 0..5 {
            assert!(tx.push(Counted::new()).is_ok());
        }
        drop(rx.pop());
        assert_eq!(LIVE.load(Ordering::SeqCst), 4);
    }
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
}

#[test]
fn spsc_full_returns_value() {
    let mut q: Spsc<u8, 2> = Spsc::new();
    let (mut tx, _rx) = q.split();
    assert!(tx.push(1).is_ok());
    assert!(tx.push(2).is_ok());
    assert_eq!(tx.push(3), Err(3));
}

#[test]
fn nested_locks_in_order_from_threads() {
    let outer: &'static Mutex<L1, u64> = Box::leak(Box::new(Mutex::new(0)));
    let inner: &'static Mutex<L2, u64> = Box::leak(Box::new(Mutex::new(0)));
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(move || {
                for _ in 0..10_000 {
                    outer.lock_with(Token::root(), |a, held| {
                        inner.lock_with(held, |b, _| {
                            *a += 1;
                            *b += 1;
                        });
                    });
                }
            });
        }
    });
    outer.lock_with(Token::root(), |a, _| assert_eq!(*a, 40_000));
    inner.lock_with(Token::root(), |b, _| assert_eq!(*b, 40_000));
}

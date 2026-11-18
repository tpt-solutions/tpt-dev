//! Model-checked tests. Run with: `RUSTFLAGS="--cfg loom" cargo test -p tpt-dev-lock --release --test loom`
#![cfg(loom)]

use loom::sync::Arc;
use loom::thread;
use tpt_dev_lock::{L1, L2, Mutex, Spsc, Token, TripleBuffer};

#[test]
fn triple_buffer_never_tears_or_goes_back() {
    loom::model(|| {
        let buf: &'static mut TripleBuffer<u32> = Box::leak(Box::new(TripleBuffer::new(0, 0, 0)));
        let (mut w, mut r) = buf.split();
        let writer = thread::spawn(move || {
            w.write(1);
            w.write(2);
        });
        let mut prev = 0;
        for _ in 0..2 {
            let seen = r.with_latest(|v| *v);
            assert!(seen >= prev);
            assert!(seen <= 2);
            prev = seen;
        }
        writer.join().expect("thread panicked");
        r.with_latest(|v| assert_eq!(*v, 2));
    });
}

#[test]
fn spsc_delivers_in_order() {
    loom::model(|| {
        let q: &'static mut Spsc<u8, 2> = Box::leak(Box::new(Spsc::new()));
        let (mut tx, mut rx) = q.split();
        let producer = thread::spawn(move || {
            for i in 0..3u8 {
                let mut v = i;
                while let Err(back) = tx.push(v) {
                    v = back;
                    thread::yield_now();
                }
            }
        });
        let mut expected = 0u8;
        while expected < 3 {
            match rx.pop() {
                Some(v) => {
                    assert_eq!(v, expected);
                    expected += 1;
                }
                None => thread::yield_now(),
            }
        }
        producer.join().expect("thread panicked");
    });
}

#[test]
fn ordered_locks_do_not_deadlock() {
    loom::model(|| {
        let outer = Arc::new(Mutex::<L1, u32>::new(0));
        let inner = Arc::new(Mutex::<L2, u32>::new(0));
        let (o2, i2) = (outer.clone(), inner.clone());
        let t = thread::spawn(move || {
            o2.lock_with(Token::root(), |a, held| {
                i2.lock_with(held, |b, _| {
                    *a += 1;
                    *b += 1;
                });
            });
        });
        outer.lock_with(Token::root(), |a, held| {
            inner.lock_with(held, |b, _| {
                *a += 1;
                *b += 1;
            });
        });
        t.join().expect("thread panicked");
        outer.lock_with(Token::root(), |a, _| assert_eq!(*a, 2));
    });
}

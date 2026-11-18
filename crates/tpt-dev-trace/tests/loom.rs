//! Model-checked tests for concurrent recording.
//! Run with: `RUSTFLAGS="--cfg loom" cargo test -p tpt-dev-trace --release --test loom`
#![cfg(loom)]

use loom::sync::Arc;
use loom::thread;
use tpt_dev_trace::{AtomicIds, Collector, ManualClock, Record, Tracer};

#[test]
fn concurrent_spans_keep_every_record() {
    loom::model(|| {
        let collector = Arc::new(Collector::<8>::new());
        let ids = Arc::new(AtomicIds::new(1));
        let clock = Arc::new(ManualClock::new(0));

        let handles: Vec<_> = (0..2)
            .map(|_| {
                let (c, i, k) = (collector.clone(), ids.clone(), clock.clone());
                thread::spawn(move || {
                    let tracer = Tracer::new(&*c, &*k, &*i);
                    tracer.start_trace("work").event("step", 1);
                })
            })
            .collect();
        for h in handles {
            h.join().expect("thread panicked");
        }

        // Two spans, each with a start, an event and an end: six records total.
        assert_eq!(collector.len(), 6);
        assert_eq!(collector.dropped(), 0);
    });
}

#[test]
fn record_is_copy_and_fixed_size() {
    // Guards the zero-allocation promise: records must stay plain data.
    fn assert_copy<T: Copy>() {}
    assert_copy::<Record>();
}

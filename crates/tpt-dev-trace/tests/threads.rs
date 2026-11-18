//! Multi-threaded recording and export on the standard library.

use std::thread;

use tpt_dev_trace::{AtomicIds, Collector, Kind, ManualClock, Record, Sink, Tracer};

struct Collect(Vec<Record>);

impl Sink for Collect {
    fn export(&mut self, records: &[Record]) {
        self.0.extend_from_slice(records);
    }
}

#[test]
fn concurrent_spans_are_all_recorded_and_paired() {
    const THREADS: usize = 4;
    const SPANS: usize = 500;
    // Each span makes three records (start, event, end), so nothing is dropped.
    // A static avoids copying a large buffer through the test thread's stack.
    static COLLECTOR: Collector<{ THREADS * SPANS * 3 }> = Collector::new();
    let collector = &COLLECTOR;
    let clock = ManualClock::new(0);
    let ids = AtomicIds::new(7);
    let tracer = Tracer::new(collector, &clock, &ids);

    thread::scope(|s| {
        for _ in 0..THREADS {
            s.spawn(|| {
                for _ in 0..SPANS {
                    let span = tracer.start_trace("work");
                    span.event("step", 1);
                }
            });
        }
    });

    assert_eq!(collector.dropped(), 0);
    let mut sink = Collect(Vec::new());
    collector.flush(&mut sink);
    let starts = sink.0.iter().filter(|r| r.kind == Kind::SpanStart).count();
    let ends = sink.0.iter().filter(|r| r.kind == Kind::SpanEnd).count();
    assert_eq!(starts, THREADS * SPANS);
    assert_eq!(ends, THREADS * SPANS);
    // Every span that started also ended, with the same span ID.
    for start in sink.0.iter().filter(|r| r.kind == Kind::SpanStart) {
        assert!(
            sink.0
                .iter()
                .any(|r| r.kind == Kind::SpanEnd && r.span == start.span)
        );
    }
}

#[test]
fn overflow_is_counted_not_blocking() {
    static COLLECTOR: Collector<4> = Collector::new();
    let collector = &COLLECTOR;
    let clock = ManualClock::new(0);
    let ids = AtomicIds::new(1);
    let tracer = Tracer::new(collector, &clock, &ids);
    thread::scope(|s| {
        for _ in 0..2 {
            s.spawn(|| {
                for _ in 0..100 {
                    tracer.start_trace("x").event("e", 0);
                }
            });
        }
    });
    assert_eq!(collector.len(), 4);
    assert_eq!(collector.dropped(), 2 * 100 * 3 - 4);
}

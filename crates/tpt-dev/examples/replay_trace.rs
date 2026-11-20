//! Record a run of an async task, replay it, and check the two agree. Spans
//! for the run go to a trace collector.
//!
//! Run with: `cargo run -p tpt-dev --example replay_trace --features rt`

use std::cell::{Cell, RefCell};
use std::pin::pin;

use tpt_dev::async_rt::Executor;
use tpt_dev::replay::{Log, Progress, Session, step};
use tpt_dev::trace::{AtomicIds, Collector, ManualClock, Tracer};

/// Runs one task that reads an input, and returns the value it read.
fn run(session: &RefCell<Session<16>>) -> u64 {
    let out = Cell::new(0);
    let mut exec = pin!(Executor::<1>::new());
    let task = pin!(async {
        out.set(session.borrow_mut().input(0).unwrap());
    });
    exec.as_mut().spawn(task, 1).unwrap();
    while let Ok(Progress::Polled { .. }) = step(session, exec.as_mut()) {}
    out.get()
}

fn main() {
    let collector: Collector<8> = Collector::new();
    let clock = ManualClock::new(0);
    let ids = AtomicIds::new(1);
    let tracer = Tracer::new(&collector, &clock, &ids);

    let recording = RefCell::new(Session::<16>::recording(7));
    let first = {
        let _span = tracer.start_trace("record");
        run(&recording)
    };
    clock.advance(10);

    let script: Log<16> = *recording.borrow().log();
    let replay = RefCell::new(Session::<16>::replaying(script));
    let second = {
        let _span = tracer.start_trace("replay");
        run(&replay)
    };

    println!("recorded value: {first}");
    println!("replayed value: {second}");
    println!("replay matches: {}", first == second);
    println!("trace records: {}", collector.len());
}

//! Measures what one sample costs to record on this host.
//!
//! The figures are for this machine only. The real cost on a target depends on
//! its clock, its memory, and the interrupt entry code around the hook.
//!
//! Run with `cargo run --release -p tpt-dev-profiler --example overhead`.

use std::hint::black_box;
use std::time::Instant;

use tpt_dev_profiler::Sampler;

const SAMPLES: usize = 4096;
const DEPTH: usize = 8;
const ROUNDS: usize = 64;

fn main() {
    let stack = [
        0x0800_1234usize,
        0x0800_0f00,
        0x0800_0100,
        0x0800_0010,
        0x0800_0008,
    ];
    let mut sampler: Box<Sampler<SAMPLES, DEPTH>> = Box::new(Sampler::new());

    // Keep the fastest round. Slower rounds are usually noise from the rest of the system.
    let mut best_ns_per_sample = f64::MAX;
    for _ in 0..ROUNDS {
        sampler.clear();
        let start = Instant::now();
        for _ in 0..SAMPLES {
            black_box(sampler.record(black_box(&stack)));
        }
        let elapsed = start.elapsed().as_nanos() as f64;
        best_ns_per_sample = best_ns_per_sample.min(elapsed / SAMPLES as f64);
    }

    println!(
        "record(): {best_ns_per_sample:.1} ns per sample, {}-frame stack, best of {ROUNDS} rounds",
        stack.len()
    );
    println!("share of one core, by sampling rate on this host:");
    for hz in [100.0_f64, 1_000.0, 10_000.0] {
        let share = best_ns_per_sample * hz / 1e9 * 100.0;
        println!("  {hz:>7.0} Hz: {share:.4}%");
    }
}

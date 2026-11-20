//! The benchmark runner used by the CI gate. It measures a fixed set of
//! workloads and writes one results file in the `Baseline` text format.
//!
//! Usage:
//!
//! ```text
//! cargo run --release -p tpt-dev-bench --example bench_suite --features std -- <results-file>
//! ```
//!
//! The gate (`examples/gate.rs`) compares this output with a committed baseline.
//! To benchmark your own crate, copy this file, replace the workloads in
//! [`suite`], and keep the output format.

use std::env;
use std::fs;
use std::process;

use tpt_dev_bench::harness::measure;
use tpt_dev_bench::{Baseline, Entry, summarise, trim_outliers};

const WARMUP: usize = 20;
const ITERS: usize = 60;

/// A workload: a closure run once per iteration.
type Workload = Box<dyn FnMut()>;

/// Each workload is a name (no spaces) and a closure run once per iteration.
fn suite() -> Vec<(&'static str, Workload)> {
    let samples: Vec<f64> = (0..4096).map(|i| f64::from(i % 97) + 0.5).collect();
    let baseline_text = {
        let mut b = Baseline::new();
        for i in 0..64 {
            b.insert(Entry {
                name: format!("bench{i}"),
                mean: 100.0 + f64::from(i),
                stddev: 1.0,
                n: 20,
            });
        }
        b.to_text()
    };

    let stats_samples = samples.clone();
    let parse_text = baseline_text.clone();
    vec![
        (
            "summarise_4096",
            Box::new(move || {
                let s = summarise(&stats_samples);
                std::hint::black_box(s);
            }),
        ),
        (
            "trim_outliers_4096",
            Box::new(move || {
                let t = trim_outliers(&samples);
                std::hint::black_box(t);
            }),
        ),
        (
            "baseline_parse_64",
            Box::new(move || {
                let b = Baseline::parse(&parse_text);
                std::hint::black_box(b.is_ok());
            }),
        ),
    ]
}

fn main() {
    let Some(out) = env::args().nth(1) else {
        eprintln!("usage: bench_suite <results-file>");
        process::exit(2);
    };

    let mut results = Baseline::new();
    for (name, mut work) in suite() {
        let raw = measure(WARMUP, ITERS, &mut *work);
        let kept = trim_outliers(&raw);
        let Some(s) = summarise(&kept) else {
            eprintln!("bench_suite: {name}: no samples left after trimming");
            process::exit(1);
        };
        results.insert(Entry {
            name: name.to_string(),
            mean: s.mean,
            stddev: s.stddev,
            n: s.n,
        });
    }

    if let Err(e) = fs::write(&out, results.to_text()) {
        eprintln!("bench_suite: cannot write {out}: {e}");
        process::exit(1);
    }
    print!("{}", results.to_text());
}

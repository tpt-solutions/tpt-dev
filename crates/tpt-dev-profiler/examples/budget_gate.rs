//! A real-time budget gate for CI.
//!
//! The gate times one control step and fails the build if either check fails:
//!
//! - the 99th percentile of the step time is above the real-time budget, or
//! - the step is a trusted regression against a committed baseline (checked
//!   with `tpt-dev-bench`).
//!
//! When the gate fails, it prints a profile of where the samples landed. The
//! profile comes from `tpt-dev-profiler`. The stage hooks below stand in for a
//! timer interrupt: a real board would call `Sampler::record` from its timer
//! handler with the interrupted program counter.
//!
//! Usage:
//!
//! ```text
//! # Create or refresh the baseline on the base branch:
//! cargo run --release -p tpt-dev-profiler --example budget_gate -- --write-baseline budget-baseline.txt
//!
//! # Gate a change (exit 0 = pass, 1 = budget or regression failure, 2 = usage error):
//! cargo run --release -p tpt-dev-profiler --example budget_gate -- --budget-ns 50000 --baseline budget-baseline.txt
//! ```
//!
//! Shared CI runners are noisy. The budget should have headroom over the
//! typical value, and the regression check uses the bench gate's statistics.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::env;
use std::fs;
use std::hint::black_box;
use std::process;

use tpt_dev_bench::harness::measure;
use tpt_dev_bench::{Baseline, Entry, Policy, Verdict, judge_entry, summarise, trim_outliers};
use tpt_dev_profiler::{Report, Sampler, Symbols};

const NAME: &str = "control_step";
const WARMUP: usize = 50;
const ITERS: usize = 400;
/// Steps per timed run. A single step is shorter than the clock's resolution on some hosts.
const BATCH: usize = 64;
const INPUT_LEN: usize = 512;
const DEFAULT_BUDGET_NS: f64 = 50_000.0;

// Program counters for the stage hooks, and the symbol table that names them.
const PC_MAIN: usize = 0x1410;
const PC_STEP: usize = 0x1310;
const PC_FILTER: usize = 0x1010;
const PC_LAW: usize = 0x1110;
const PC_COMMIT: usize = 0x1210;
const SYMBOLS: &[(usize, &str)] = &[
    (0x1000, "filter"),
    (0x1100, "control_law"),
    (0x1200, "commit"),
    (0x1300, "control_step"),
    (0x1400, "main"),
];
const CODE_END: usize = 0x1500;

/// Stage 1: fold the inputs that pass a mask.
fn filter(input: &[u32]) -> u32 {
    input
        .iter()
        .filter(|&&v| v & 3 != 0)
        .fold(0u32, |acc, &v| acc.wrapping_add(v))
}

/// Stage 2: a fixed-length update loop, so the cost does not depend on the data.
fn control_law(mut x: u32) -> u32 {
    for _ in 0..64 {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    }
    x
}

/// Stage 3: output mixing.
fn commit(x: u32) -> u32 {
    x ^ (x >> 7)
}

/// One control step, with a stage hook before each stage.
fn step(sampler: &Sampler<8192, 4>, input: &[u32]) -> u32 {
    let _ = sampler.record(&[PC_FILTER, PC_STEP, PC_MAIN]);
    let a = filter(input);
    let _ = sampler.record(&[PC_LAW, PC_STEP, PC_MAIN]);
    let b = control_law(a);
    let _ = sampler.record(&[PC_COMMIT, PC_STEP, PC_MAIN]);
    commit(b)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx]
}

struct Args {
    budget_ns: f64,
    baseline: Option<String>,
    write_baseline: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        budget_ns: DEFAULT_BUDGET_NS,
        baseline: None,
        write_baseline: None,
    };
    let mut it = env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--budget-ns" => {
                args.budget_ns = value
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .ok_or_else(|| format!("--budget-ns needs a positive number, got {value}"))?;
            }
            "--baseline" => args.baseline = Some(value),
            "--write-baseline" => args.write_baseline = Some(value),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(args)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("budget_gate: {msg}");
            eprintln!(
                "usage: budget_gate [--budget-ns N] [--baseline FILE] [--write-baseline FILE]"
            );
            process::exit(2);
        }
    };

    let input: Vec<u32> = (0..INPUT_LEN as u32)
        .map(|i| i.wrapping_mul(2_654_435_761))
        .collect();
    let sampler: Box<Sampler<8192, 4>> = Box::new(Sampler::new());
    // Each timed run is a batch of steps. The per-step times are the batch times divided by BATCH.
    let batch_times = measure(WARMUP, ITERS, || {
        let mut acc = 0u32;
        for _ in 0..BATCH {
            acc ^= step(&sampler, black_box(&input));
        }
        black_box(acc)
    });
    let times: Vec<f64> = batch_times.iter().map(|t| t / BATCH as f64).collect();

    let kept = trim_outliers(&times);
    let Some(summary) = summarise(&kept) else {
        eprintln!("budget_gate: not enough samples after trimming");
        process::exit(2);
    };
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let p99 = percentile(&sorted, 0.99);
    println!(
        "{NAME}: mean {:.0} ns, median {:.0} ns, p99 {p99:.0} ns, n {} (budget {:.0} ns)",
        summary.mean, summary.median, summary.n, args.budget_ns
    );

    let current = Entry {
        name: NAME.into(),
        mean: summary.mean,
        stddev: summary.stddev,
        n: summary.n,
    };

    if let Some(path) = &args.write_baseline {
        let mut baseline = Baseline::new();
        baseline.insert(current);
        if let Err(e) = fs::write(path, baseline.to_text()) {
            eprintln!("budget_gate: cannot write {path}: {e}");
            process::exit(2);
        }
        println!("wrote baseline to {path}");
        return;
    }

    let mut failed = false;
    if p99 > args.budget_ns {
        failed = true;
        println!(
            "BUDGET: p99 {p99:.0} ns is over the {:.0} ns budget",
            args.budget_ns
        );
    } else {
        println!("budget: ok");
    }

    if let Some(path) = &args.baseline {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("budget_gate: cannot read {path}: {e}");
                process::exit(2);
            }
        };
        let baseline = match Baseline::parse(&text) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("budget_gate: {path}: {e}");
                process::exit(2);
            }
        };
        let Some(base) = baseline.get(NAME) else {
            eprintln!("budget_gate: {path} has no entry for {NAME}");
            process::exit(2);
        };
        match judge_entry(&current, base, Policy::default()) {
            Some(Verdict::Regression { change_pct }) => {
                failed = true;
                println!("REGRESSION: {change_pct:+.1}% against the baseline");
            }
            Some(Verdict::Improvement { change_pct }) => {
                println!("regression check: improvement {change_pct:+.1}%");
            }
            Some(Verdict::Inconclusive { change_pct }) => {
                println!("regression check: inconclusive {change_pct:+.1}% (not trusted)");
            }
            Some(Verdict::Ok) => println!("regression check: ok"),
            None => {
                eprintln!("budget_gate: cannot judge against the baseline");
                process::exit(2);
            }
        }
    }

    if failed {
        // Show where the samples landed, to help find the slow stage.
        let Some(symbols) = Symbols::new(SYMBOLS, CODE_END) else {
            eprintln!("budget_gate: bad symbol table");
            process::exit(2);
        };
        let report = Report::new(&sampler, &symbols);
        let mut text = String::new();
        if report.write_text(&mut text).is_ok() {
            println!("profile of the measured steps:\n{text}");
        }
        process::exit(1);
    }
}

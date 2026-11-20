//! The CI gate as a command. Compares a results file with a baseline file.
//!
//! Usage:
//!
//! ```text
//! cargo run -p tpt-dev-bench --example gate -- <baseline> <results> [--max-regression-pct PCT] [--critical-t T]
//! ```
//!
//! Both files use the `Baseline` text format (`name mean stddev n` per line).
//! Exit codes:
//!
//! - `0`: no trusted regression. Improvements and inconclusive changes pass.
//! - `1`: at least one benchmark has a trusted regression beyond the policy.
//! - `2`: usage error, unreadable file, parse error, or a benchmark the gate cannot judge.

use std::fs;
use tpt_dev_bench::{Baseline, Policy, Verdict, judge_entry};

const USAGE: &str = "usage: gate <baseline> <results> [--max-regression-pct PCT] [--critical-t T]";

/// Runs the gate on `args` (without the program name). Returns the exit code.
pub(crate) fn run(args: &[String]) -> i32 {
    let (paths, policy) = match parse_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("gate: {msg}\n{USAGE}");
            return 2;
        }
    };
    let baseline = match load(&paths.0) {
        Ok(b) => b,
        Err(msg) => {
            eprintln!("gate: {msg}");
            return 2;
        }
    };
    let results = match load(&paths.1) {
        Ok(b) => b,
        Err(msg) => {
            eprintln!("gate: {msg}");
            return 2;
        }
    };
    if results.entries().is_empty() {
        eprintln!("gate: results file {} has no benchmarks", paths.1);
        return 2;
    }

    let mut failed = false;
    let mut errors = false;
    for cur in results.entries() {
        let Some(base) = baseline.get(&cur.name) else {
            println!("{}: no baseline entry, not judged", cur.name);
            continue;
        };
        match judge_entry(cur, base, policy) {
            Some(Verdict::Regression { change_pct }) => {
                failed = true;
                println!("{}: REGRESSION {change_pct:+.1}%", cur.name);
            }
            Some(Verdict::Improvement { change_pct }) => {
                println!("{}: improvement {change_pct:+.1}%", cur.name);
            }
            Some(Verdict::Inconclusive { change_pct }) => {
                println!("{}: inconclusive {change_pct:+.1}% (not trusted)", cur.name);
            }
            Some(Verdict::Ok) => println!("{}: ok", cur.name),
            None => {
                errors = true;
                eprintln!(
                    "gate: {}: cannot judge (need at least 2 samples and a non-zero baseline mean)",
                    cur.name
                );
            }
        }
    }

    if errors {
        2
    } else if failed {
        1
    } else {
        0
    }
}

fn parse_args(args: &[String]) -> Result<((String, String), Policy), String> {
    let mut policy = Policy::default();
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--max-regression-pct" => {
                policy.max_regression_pct = number(it.next(), arg)?;
            }
            "--critical-t" => {
                policy.critical_t = Some(number(it.next(), arg)?);
            }
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            path => files.push(path.to_string()),
        }
    }
    match <[String; 2]>::try_from(files) {
        Ok([base, results]) => Ok(((base, results), policy)),
        Err(files) => Err(format!("expected 2 files, got {}", files.len())),
    }
}

fn number(value: Option<&String>, flag: &str) -> Result<f64, String> {
    let text = value.ok_or_else(|| format!("{flag} needs a value"))?;
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .ok_or_else(|| format!("{flag} needs a non-negative number, got {text}"))
}

fn load(path: &str) -> Result<Baseline, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    Baseline::parse(&text).map_err(|e| format!("{path}: {e}"))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run(&args));
}

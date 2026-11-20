//! Cross-check against externally produced QF_LRA benchmarks.
//!
//! The benchmarks are not stored in this repository. Point `TPT_SMTLIB_DIR`
//! at a directory of `.smt2` files that carry a `(set-info :status sat)` or
//! `(set-info :status unsat)` line, then run:
//!
//! ```text
//! TPT_SMTLIB_DIR=/path/to/benchmarks cargo test -p tpt-dev-solver \
//!     --test smtlib_external -- --ignored --nocapture
//! ```
//!
//! The directory is searched recursively. Files larger than
//! `TPT_SMTLIB_MAX_BYTES` (default 20000) are skipped, and every check runs
//! under a work budget, so a hard instance is reported as `unknown` instead of
//! running for a long time. A file the reader does not support (for example
//! `Int` sorts, `ite`, or `check-sat-assuming`) is reported as `unsupported`.
//!
//! Only a definite answer that contradicts the expected status fails the test.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use tpt_dev_solver::{Budget, Session};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "smt2") {
            out.push(path);
        }
    }
}

/// The status from the first `(set-info :status ...)` line, if any.
fn expected_status(script: &str) -> Option<&'static str> {
    for line in script.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("(set-info :status") {
            let word = rest.trim().trim_end_matches(')').trim();
            return match word {
                "sat" => Some("sat"),
                "unsat" => Some("unsat"),
                _ => None,
            };
        }
    }
    None
}

#[derive(Default)]
struct Tally {
    agree: usize,
    unknown: usize,
    unsupported: usize,
    reasons: Vec<String>,
    skipped_size: usize,
    no_status: usize,
    disagree: Vec<String>,
}

#[test]
#[ignore = "needs TPT_SMTLIB_DIR; see the module docs"]
fn external_benchmarks_agree_with_their_status() {
    let Ok(dir) = std::env::var("TPT_SMTLIB_DIR") else {
        panic!("set TPT_SMTLIB_DIR to a directory of .smt2 benchmarks");
    };
    let max_bytes: u64 = std::env::var("TPT_SMTLIB_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);

    let mut files = Vec::new();
    collect(Path::new(&dir), &mut files);
    files.sort();

    let budget = Budget {
        max_formula_nodes: 2_000_000,
        max_clauses: 2_000_000,
        max_pivots: 5_000_000,
    };

    let mut tally = Tally::default();
    for path in &files {
        let Ok(meta) = fs::metadata(path) else {
            continue;
        };
        if meta.len() > max_bytes {
            tally.skipped_size += 1;
            continue;
        }
        let script = fs::read_to_string(path).unwrap();
        let Some(want) = expected_status(&script) else {
            tally.no_status += 1;
            continue;
        };

        let mut session = Session::new();
        session.set_budget(budget);
        match session.run(&script) {
            Ok(out) => {
                let got = out.lines().next().unwrap_or("");
                if got == want {
                    tally.agree += 1;
                } else {
                    tally.disagree.push(format!(
                        "{}: expected {want}, solver said {got:?}",
                        path.display()
                    ));
                }
            }
            Err(e) if e.message.starts_with("unknown") => tally.unknown += 1,
            Err(e) => {
                tally.unsupported += 1;
                tally
                    .reasons
                    .push(format!("{}: {}", path.display(), e.message));
            }
        }
    }

    println!(
        "benchmarks found: {}, agree: {}, disagree: {}, unknown: {}, \
         unsupported: {}, skipped (over {max_bytes} bytes): {}, no status: {}",
        files.len(),
        tally.agree,
        tally.disagree.len(),
        tally.unknown,
        tally.unsupported,
        tally.skipped_size,
        tally.no_status,
    );
    for r in &tally.reasons {
        println!("unsupported {r}");
    }
    assert!(
        tally.disagree.is_empty(),
        "solver disagrees with benchmark status:\n{}",
        tally.disagree.join("\n")
    );
}

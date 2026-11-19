//! Exit-code tests for the `gate` example. The example file is compiled into
//! this test crate, so each case calls the same `run` function the binary uses.
#![allow(clippy::unwrap_used)]

#[allow(dead_code)]
#[path = "../examples/gate.rs"]
mod gate_cli;

use std::fs;
use std::path::PathBuf;
use std::process;

/// A fresh directory under the system temp dir, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(case: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("tpt-bench-gate-{}-{case}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str, text: &str) -> String {
        let path = self.0.join(name);
        fs::write(&path, text).unwrap();
        path.to_string_lossy().into_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const BASELINE: &str = "# name mean stddev n\nparse 100 1 20\nencode 50 0.5 20\n";

fn gate(case: &str, baseline: &str, results: &str) -> i32 {
    let dir = TempDir::new(case);
    let base = dir.file("baseline.txt", baseline);
    let res = dir.file("results.txt", results);
    gate_cli::run(&[base, res])
}

#[test]
fn no_regression_exits_zero() {
    let code = gate(
        "no-regression",
        BASELINE,
        "# name mean stddev n\nparse 101 1 20\nencode 50.2 0.5 20\n",
    );
    assert_eq!(code, 0);
}

#[test]
fn speedup_exits_zero() {
    let code = gate("speedup", BASELINE, "parse 60 1 20\nencode 50 0.5 20\n");
    assert_eq!(code, 0);
}

#[test]
fn trusted_regression_exits_one() {
    let code = gate("regression", BASELINE, "parse 130 1 20\nencode 50 0.5 20\n");
    assert_eq!(code, 1);
}

#[test]
fn bad_baseline_file_exits_two() {
    let code = gate("bad-baseline", "parse banana 1 20\n", "parse 100 1 20\n");
    assert_eq!(code, 2);
}

#[test]
fn bad_results_file_exits_two() {
    let code = gate("bad-results", BASELINE, "parse 100 1\n");
    assert_eq!(code, 2);
}

#[test]
fn missing_file_exits_two() {
    let dir = TempDir::new("missing");
    let base = dir.file("baseline.txt", BASELINE);
    let missing = dir.0.join("nope.txt").to_string_lossy().into_owned();
    assert_eq!(gate_cli::run(&[base, missing]), 2);
}

#[test]
fn usage_errors_exit_two() {
    assert_eq!(gate_cli::run(&[]), 2);
    assert_eq!(gate_cli::run(&["only-one".to_string()]), 2);
    let dir = TempDir::new("usage");
    let base = dir.file("baseline.txt", BASELINE);
    let res = dir.file("results.txt", "parse 100 1 20\n");
    assert_eq!(
        gate_cli::run(&[base.clone(), res.clone(), "--max-regression-pct".into()]),
        2
    );
    assert_eq!(
        gate_cli::run(&[
            base.clone(),
            res.clone(),
            "--max-regression-pct".into(),
            "x".into()
        ]),
        2
    );
    assert_eq!(gate_cli::run(&[base, res, "--bogus".into()]), 2);
}

#[test]
fn empty_results_exit_two() {
    assert_eq!(gate("empty-results", BASELINE, "# nothing\n"), 2);
}

#[test]
fn unjudgeable_entry_exits_two() {
    // n = 1 leaves no variance estimate, so the gate cannot judge it.
    assert_eq!(gate("one-sample", BASELINE, "parse 100 0 1\n"), 2);
}

#[test]
fn policy_flags_change_the_verdict() {
    let dir = TempDir::new("policy");
    let base = dir.file("baseline.txt", BASELINE);
    let res = dir.file("results.txt", "parse 130 1 20\n");
    // A 30% slowdown passes when the threshold is raised above it.
    let args = [base, res, "--max-regression-pct".into(), "50".into()];
    assert_eq!(gate_cli::run(&args), 0);
}

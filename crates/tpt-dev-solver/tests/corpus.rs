//! Runs the hand-checked SMT-LIB QF_LRA corpus in `tests/corpus/`.
//!
//! Each file lists the expected answer to each `check-sat`, in order, on
//! comment lines of the form `; expect: sat` or `; expect: unsat`. The answers
//! were worked out by hand, not taken from another solver, so a disagreement
//! means either the solver or the expectation is wrong. Read the file before
//! changing either.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use tpt_dev_solver::Session;

fn corpus_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "smt2"))
        .collect();
    files.sort();
    files
}

/// The expected answers, in order, from the `; expect:` comment lines.
fn expectations(script: &str) -> Vec<String> {
    script
        .lines()
        .filter_map(|line| line.trim().strip_prefix("; expect:"))
        .map(|answer| answer.trim().to_string())
        .collect()
}

#[test]
fn corpus_answers_match_the_hand_checked_expectations() {
    let files = corpus_files();
    assert!(files.len() >= 20, "corpus has only {} files", files.len());

    let mut failures = Vec::new();
    for path in &files {
        let script = fs::read_to_string(path).unwrap();
        let expected = expectations(&script);
        assert!(
            !expected.is_empty(),
            "{} has no expectations",
            path.display()
        );

        let mut session = Session::default();
        let output = match session.run(&script) {
            Ok(out) => out,
            Err(e) => {
                failures.push(format!("{}: script error: {e:?}", path.display()));
                continue;
            }
        };
        let got: Vec<&str> = output.lines().collect();
        let want: Vec<&str> = expected.iter().map(String::as_str).collect();
        if got != want {
            failures.push(format!(
                "{}: expected {want:?}, got {got:?}",
                path.display()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "corpus failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_expectation_is_sat_or_unsat() {
    for path in corpus_files() {
        let script = fs::read_to_string(&path).unwrap();
        for answer in expectations(&script) {
            assert!(
                answer == "sat" || answer == "unsat",
                "{}: bad expectation {answer:?}",
                path.display()
            );
        }
    }
}

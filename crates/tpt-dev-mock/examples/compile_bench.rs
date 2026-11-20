//! Compile-time benchmark: `#[mock]` against mockall's `#[automock]`.
//!
//! For each trait size it generates two scratch crates with the same trait
//! (N methods taking `(u8, u16)` and returning `u32`) and one test per method
//! that sets an expectation, makes the call and verifies it. It then times:
//!
//! - `cold`: the first `cargo test --no-run`, which also compiles dependencies
//!   (for mockall this includes mockall and its dependencies);
//! - `rebuild`: the median of several `cargo test --no-run` runs after a
//!   source edit, which recompiles only the scratch crate (dependencies warm).
//!
//! Usage (from the repository root):
//!
//! ```text
//! cargo run --release -p tpt-dev-mock --example compile_bench -- [scratch-dir] [sizes] [reps]
//! ```
//!
//! `scratch-dir` defaults to `<temp>/tpt-dev-mock-compile-bench` and is
//! recreated on each run. `sizes` is a comma-separated list of method counts
//! (default `10,50`) and `reps` is the number of timed rebuilds per library (default `9`).
//! Timings depend on the machine, so compare ratios, not absolute numbers.

#![allow(clippy::print_stdout, clippy::expect_used, clippy::panic)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const EDITION: &str = "2024";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let root = args.first().map_or_else(
        || env::temp_dir().join("tpt-dev-mock-compile-bench"),
        PathBuf::from,
    );
    let sizes: Vec<usize> = args
        .get(1)
        .map_or("10,50", String::as_str)
        .split(',')
        .map(|s| s.trim().parse().expect("sizes must be integers"))
        .collect();
    let reps: usize = args
        .get(2)
        .map_or(9, |s| s.parse().expect("reps must be an integer"));

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mock_path = manifest.to_string_lossy().replace('\\', "/");
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());

    println!("scratch dir: {}", root.display());
    println!("interleaved rebuilds per library: {reps}");
    println!();
    println!(
        "{:>8} | {:>13} | {:>10} | {:>10} | {:>10} | {:>8}",
        "methods", "library", "cold (s)", "median (s)", "min (s)", "vs tpt"
    );
    println!("{}", "-".repeat(78));

    for n in sizes {
        let tpt_dir = root.join(format!("tpt-dev-mock-{n}"));
        let mockall_dir = root.join(format!("mockall-{n}"));
        write_project(
            &tpt_dir,
            "tpt-dev-mock",
            &mock_path,
            &tpt_lib(n),
            &tpt_tests(n),
        );
        write_project(
            &mockall_dir,
            "mockall",
            &mock_path,
            &mockall_lib(n),
            &mockall_tests(n),
        );

        // Cold builds include the dependencies, so each library builds from scratch.
        let tpt_cold = time_build(&cargo, &tpt_dir, None);
        let mockall_cold = time_build(&cargo, &mockall_dir, None);

        // Interleave the timed rebuilds so both libraries see the same machine load.
        let mut tpt_times = Vec::new();
        let mut mockall_times = Vec::new();
        for k in 0..reps.max(1) {
            tpt_times.push(time_build(&cargo, &tpt_dir, Some(k)));
            mockall_times.push(time_build(&cargo, &mockall_dir, Some(k)));
        }

        let rows = [
            ("tpt-dev-mock", tpt_cold, tpt_times),
            ("mockall", mockall_cold, mockall_times),
        ];
        let tpt_median = median(&rows[0].2);
        for (name, cold, times) in rows {
            let ratio = median(&times).as_secs_f64() / tpt_median.as_secs_f64();
            println!(
                "{:>8} | {:>13} | {:>10.2} | {:>10.2} | {:>10.2} | {:>7.2}x",
                n,
                name,
                cold.as_secs_f64(),
                median(&times).as_secs_f64(),
                times.iter().min().map_or(0.0, Duration::as_secs_f64),
                ratio
            );
        }
        println!("{}", "-".repeat(78));
    }
}

/// The median of a non-empty list of durations.
fn median(times: &[Duration]) -> Duration {
    let mut sorted = times.to_vec();
    sorted.sort();
    sorted[sorted.len() / 2]
}

/// `#[mock]` version of the trait.
fn tpt_lib(n: usize) -> String {
    let mut s = String::from("use tpt_dev_mock::mock;\n\n#[mock]\npub trait Target {\n");
    for i in 0..n {
        s.push_str(&format!("    fn m{i}(&self, a: u8, b: u16) -> u32;\n"));
    }
    s.push_str("}\n");
    s
}

/// One test per method, each with an expectation, a call and `verify`.
fn tpt_tests(n: usize) -> String {
    let mut s = String::from("use bench::{MockTarget, Target};\n\n");
    for i in 0..n {
        s.push_str(&format!(
            "#[test]\nfn t{i}() {{\n    let m = MockTarget::new();\n    \
             m.m{i}.expect().times(1).returns(7);\n    let _ = m.m{i}(1, 2);\n    m.verify();\n}}\n\n"
        ));
    }
    s
}

/// `#[automock]` version of the same trait.
fn mockall_lib(n: usize) -> String {
    let mut s = String::from("use mockall::automock;\n\n#[automock]\npub trait Target {\n");
    for i in 0..n {
        s.push_str(&format!("    fn m{i}(&self, a: u8, b: u16) -> u32;\n"));
    }
    s.push_str("}\n");
    s
}

/// Matching tests written against mockall's API.
fn mockall_tests(n: usize) -> String {
    let mut s = String::from("use bench::{MockTarget, Target};\n\n");
    for i in 0..n {
        s.push_str(&format!(
            "#[test]\nfn t{i}() {{\n    let mut m = MockTarget::new();\n    \
             m.expect_m{i}().times(1).return_const(7u32);\n    let _ = m.m{i}(1, 2);\n}}\n\n"
        ));
    }
    s
}

fn write_project(dir: &Path, kind: &str, mock_path: &str, lib: &str, tests: &str) {
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir.join("src")).expect("create scratch project");
    fs::create_dir_all(dir.join("tests")).expect("create scratch tests dir");
    let deps = if kind == "mockall" {
        "mockall = \"0.15\"\n".to_owned()
    } else {
        format!("tpt-dev-mock = {{ path = \"{mock_path}\" }}\n")
    };
    let manifest = format!(
        "[package]\nname = \"bench\"\nversion = \"0.0.0\"\nedition = \"{EDITION}\"\n\n\
         [lib]\npath = \"src/lib.rs\"\n\n[dependencies]\n{deps}"
    );
    fs::write(dir.join("Cargo.toml"), manifest).expect("write Cargo.toml");
    fs::write(dir.join("src/lib.rs"), lib).expect("write lib.rs");
    fs::write(dir.join("tests/uses.rs"), tests).expect("write tests");
}

/// Runs `cargo test --no-run` once and returns how long it took.
/// `edit` is a counter appended to the library so cargo sees a change.
fn time_build(cargo: &str, dir: &Path, edit: Option<usize>) -> Duration {
    if let Some(k) = edit {
        let lib = dir.join("src/lib.rs");
        let mut text = fs::read_to_string(&lib).expect("read lib.rs");
        text.push_str(&format!("\n// edit {k}\n"));
        fs::write(&lib, text).expect("edit lib.rs");
    }
    let start = Instant::now();
    let status = Command::new(cargo)
        .args(["test", "--no-run", "-q"])
        .current_dir(dir)
        .status()
        .expect("run cargo");
    let elapsed = start.elapsed();
    assert!(
        status.success(),
        "cargo test --no-run failed in {}",
        dir.display()
    );
    elapsed
}

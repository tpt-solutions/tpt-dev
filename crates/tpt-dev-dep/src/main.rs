//! `tpt-dev-dep`: analyse a Cargo.lock file.
//!
//! Usage: `tpt-dev-dep <Cargo.lock> [licenses.txt] [--allow ID]...`
//!
//! `licenses.txt` has one `name license-expression` pair per line. The exit
//! status is 1 if there is a cycle or a license violation, and 2 on a usage error.

use std::process::ExitCode;

use tpt_dev_dep::{Policy, analyse, parse_lock};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut lock_path = None;
    let mut licenses_path = None;
    let mut allow = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--allow" => match args.get(i + 1) {
                Some(id) => {
                    allow.push(id.clone());
                    i += 1;
                }
                None => return usage("--allow needs a license identifier"),
            },
            other if lock_path.is_none() => lock_path = Some(other.to_string()),
            other if licenses_path.is_none() => licenses_path = Some(other.to_string()),
            _ => return usage("too many arguments"),
        }
        i += 1;
    }
    let Some(lock_path) = lock_path else {
        return usage("missing Cargo.lock path");
    };
    let text = match std::fs::read_to_string(&lock_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {lock_path}: {e}");
            return ExitCode::from(2);
        }
    };
    let graph = match parse_lock(&text) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{lock_path}: {e}");
            return ExitCode::from(2);
        }
    };
    let licenses: Vec<(String, String)> = match licenses_path {
        None => Vec::new(),
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(t) => t
                .lines()
                .filter_map(|l| {
                    let mut parts = l.trim().splitn(2, char::is_whitespace);
                    Some((parts.next()?.to_string(), parts.next()?.trim().to_string()))
                })
                .collect(),
            Err(e) => {
                eprintln!("cannot read {path}: {e}");
                return ExitCode::from(2);
            }
        },
    };

    let report = analyse(&graph, &licenses, &Policy { allow });
    for cycle in &report.cycles {
        println!("cycle: {}", cycle.join(" -> "));
    }
    for (name, license) in &report.license_violations {
        println!("license: {name} is {license}");
    }
    println!("largest transitive trees:");
    for (name, count) in report.transitive.iter().take(5) {
        println!("  {name}: {count}");
    }
    if report.is_clean() {
        println!("ok: {} packages", graph.packages.len());
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("tpt-dev-dep: {msg}");
    eprintln!("usage: tpt-dev-dep <Cargo.lock> [licenses.txt] [--allow ID]...");
    ExitCode::from(2)
}

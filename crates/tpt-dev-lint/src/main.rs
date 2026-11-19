//! `tpt-dev-lint`: lint Rust files with the project rules.
//!
//! Usage: `tpt-dev-lint [--config FILE] <file.rs>...`
//!
//! Exits 1 if any finding is at deny severity, and 2 on a usage or read error.

use std::process::ExitCode;

use tpt_dev_lint::{Config, Linter, default_rules, has_errors};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut config_path = None;
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--config" {
            match args.get(i + 1) {
                Some(p) => config_path = Some(p.clone()),
                None => return usage("--config needs a file"),
            }
            i += 2;
        } else {
            files.push(args[i].clone());
            i += 1;
        }
    }
    if files.is_empty() {
        return usage("no files given");
    }

    let mut linter = Linter::new(default_rules());
    if let Some(path) = config_path {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("cannot read {path}: {e}");
                return ExitCode::from(2);
            }
        };
        let config = match Config::parse(&text) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{path}: {e}");
                return ExitCode::from(2);
            }
        };
        if let Err(e) = linter.configure(&config) {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    }

    let mut all = Vec::new();
    for file in &files {
        let source = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cannot read {file}: {e}");
                return ExitCode::from(2);
            }
        };
        match linter.lint_source(&source) {
            Ok(diags) => {
                for d in &diags {
                    println!("{file}:{}:{} {d}", d.line, d.column);
                }
                all.extend(diags);
            }
            Err(e) => {
                eprintln!("{file}: {e}");
                return ExitCode::from(2);
            }
        }
    }
    if has_errors(&all) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("tpt-dev-lint: {msg}");
    eprintln!("usage: tpt-dev-lint [--config FILE] <file.rs>...");
    ExitCode::from(2)
}

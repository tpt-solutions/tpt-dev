//! `tpt-dev-dep`: analyse a Cargo.lock file or `cargo metadata` output.
//!
//! Usage:
//!
//! ```text
//! tpt-dev-dep <Cargo.lock> [licenses.txt] [--allow ID]...
//! tpt-dev-dep --metadata <metadata.json> [licenses.txt] [--allow ID]...
//!             [--include-dev] [--include-build]
//! ```
//!
//! `licenses.txt` has one `name license-expression` pair per line. The exit
//! status is 1 if there is a cycle or a license violation, and 2 on a usage error.
//! Dev and build edges come only from `--metadata`, and are followed only with
//! `--include-dev` or `--include-build`.

use std::process::ExitCode;

use tpt_dev_dep::{Graph, Policy, Scope, analyse, parse_lock, parse_metadata};

const USAGE: &str = "usage: tpt-dev-dep <Cargo.lock> [licenses.txt] [--allow ID]...\n       \
                     tpt-dev-dep --metadata <metadata.json> [licenses.txt] [--allow ID]... \
                     [--include-dev] [--include-build]";

/// What the command line asked for.
#[derive(Default)]
struct Args {
    lock_path: Option<String>,
    metadata_path: Option<String>,
    licenses_path: Option<String>,
    allow: Vec<String>,
    scope: Scope,
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(msg) => return usage(&msg),
    };

    let graph = match load_graph(&args) {
        Ok(g) => g,
        Err(code) => return code,
    };
    let licenses = match read_licenses(args.licenses_path.as_deref()) {
        Ok(l) => l,
        Err(code) => return code,
    };

    let report = analyse(&graph, &licenses, &Policy { allow: args.allow }, args.scope);
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

fn parse_args(raw: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args::default();
    let mut positional = Vec::new();
    let mut raw = raw;
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--allow" => match raw.next() {
                Some(id) => args.allow.push(id),
                None => return Err(String::from("--allow needs a license identifier")),
            },
            "--metadata" => match raw.next() {
                Some(path) => args.metadata_path = Some(path),
                None => return Err(String::from("--metadata needs a file path")),
            },
            "--include-dev" => args.scope.dev = true,
            "--include-build" => args.scope.build = true,
            other if other.starts_with("--") => {
                return Err(format!("unknown option {other}"));
            }
            _ => positional.push(arg),
        }
    }
    let mut positional = positional.into_iter();
    if args.metadata_path.is_some() {
        args.licenses_path = positional.next();
    } else {
        args.lock_path = positional.next();
        args.licenses_path = positional.next();
    }
    if positional.next().is_some() {
        return Err(String::from("too many arguments"));
    }
    if args.metadata_path.is_none() && args.lock_path.is_none() {
        return Err(String::from("missing Cargo.lock path"));
    }
    if args.metadata_path.is_some() && args.lock_path.is_some() {
        return Err(String::from(
            "give a Cargo.lock path or --metadata, not both",
        ));
    }
    Ok(args)
}

/// Reads and parses the graph source the arguments name. On failure, prints
/// the reason and returns the exit code.
fn load_graph(args: &Args) -> Result<Graph, ExitCode> {
    if let Some(path) = &args.metadata_path {
        let text = read_file(path)?;
        return parse_metadata(&text).map_err(|e| {
            eprintln!("{path}: {e}");
            ExitCode::from(2)
        });
    }
    let path = args.lock_path.as_deref().unwrap_or_default();
    let text = read_file(path)?;
    parse_lock(&text).map_err(|e| {
        eprintln!("{path}: {e}");
        ExitCode::from(2)
    })
}

fn read_licenses(path: Option<&str>) -> Result<Vec<(String, String)>, ExitCode> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let text = read_file(path)?;
    Ok(text
        .lines()
        .filter_map(|l| {
            let mut parts = l.trim().splitn(2, char::is_whitespace);
            Some((parts.next()?.to_string(), parts.next()?.trim().to_string()))
        })
        .collect())
}

fn read_file(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("cannot read {path}: {e}");
        ExitCode::from(2)
    })
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("tpt-dev-dep: {msg}");
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

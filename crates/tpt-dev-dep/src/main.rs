//! `tpt-dev-dep`: analyse a Cargo.lock file or `cargo metadata` output.
//!
//! Usage:
//!
//! ```text
//! tpt-dev-dep <Cargo.lock> [licenses.txt] [--allow ID]...
//! tpt-dev-dep --metadata <metadata.json> [licenses.txt] [--allow ID]...
//!             [--include-dev] [--include-build] [--json]
//! ```
//!
//! With `--metadata` and no `licenses.txt`, licenses come from each package's
//! `license` field. `--json` prints the report as one JSON object. The exit
//! status is 1 if there is a cycle or a license violation, and 2 on a usage error.
//! Dev and build edges come only from `--metadata`, and are followed only with
//! `--include-dev` or `--include-build`.

use std::process::ExitCode;

use tpt_dev_dep::{
    Graph, Policy, Scope, analyse, parse_lock, parse_metadata, parse_metadata_licenses,
};

const USAGE: &str = "usage: tpt-dev-dep <Cargo.lock> [licenses.txt] [--allow ID]...\n       \
                     tpt-dev-dep --metadata <metadata.json> [licenses.txt] [--allow ID]... \
                     [--include-dev] [--include-build] [--json]";

/// What the command line asked for.
#[derive(Default)]
struct Args {
    lock_path: Option<String>,
    metadata_path: Option<String>,
    licenses_path: Option<String>,
    allow: Vec<String>,
    scope: Scope,
    json: bool,
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
    let licenses = match (&args.licenses_path, &args.metadata_path) {
        // With --metadata and no licenses.txt, take the licenses from cargo metadata.
        (None, Some(path)) => match read_file(path).and_then(|text| {
            parse_metadata_licenses(&text).map_err(|e| {
                eprintln!("{path}: {e}");
                ExitCode::from(2)
            })
        }) {
            Ok(l) => l,
            Err(code) => return code,
        },
        _ => match read_licenses(args.licenses_path.as_deref()) {
            Ok(l) => l,
            Err(code) => return code,
        },
    };

    let report = analyse(&graph, &licenses, &Policy { allow: args.allow }, args.scope);
    if args.json {
        println!("{}", report_json(&graph, &report));
    } else {
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
        }
    }
    if report.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Renders the report as one JSON object. Every package appears in
/// `transitive`, largest first, so tools can pick their own threshold.
fn report_json(graph: &Graph, report: &tpt_dev_dep::Report) -> String {
    let cycles: Vec<String> = report
        .cycles
        .iter()
        .map(|c| {
            format!(
                "[{}]",
                c.iter().map(|n| json_str(n)).collect::<Vec<_>>().join(",")
            )
        })
        .collect();
    let violations: Vec<String> = report
        .license_violations
        .iter()
        .map(|(name, license)| {
            format!(
                "{{\"name\":{},\"license\":{}}}",
                json_str(name),
                json_str(license)
            )
        })
        .collect();
    let transitive: Vec<String> = report
        .transitive
        .iter()
        .map(|(name, count)| format!("{{\"name\":{},\"count\":{count}}}", json_str(name)))
        .collect();
    format!(
        "{{\"ok\":{},\"packages\":{},\"cycles\":[{}],\"license_violations\":[{}],\"transitive\":[{}]}}",
        report.is_clean(),
        graph.packages.len(),
        cycles.join(","),
        violations.join(","),
        transitive.join(","),
    )
}

/// A JSON string literal, with the escapes JSON requires.
fn json_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
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
            "--json" => args.json = true,
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

#[cfg(test)]
mod tests {
    use super::json_str;

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json_str("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(json_str("line\nbreak\u{1}"), "\"line\\nbreak\\u0001\"");
    }
}

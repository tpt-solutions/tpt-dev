//! `cargo tpt`: one command for the tpt-dev project checks.
//!
//! The subcommands run cargo and the other tpt-dev tools as subprocesses, so
//! they always use the versions in this workspace:
//!
//! ```text
//! cargo tpt check [--full]               fmt, clippy, lint, dependency policy (--full adds tests and cargo-deny)
//! cargo tpt lint [--config FILE] [FILE]  project lint rules, over every .rs file under crates/ by default
//! cargo tpt dep [ARGS]                   dependency policy from cargo metadata, with the deny.toml allow list
//! cargo tpt headers SRC GUARD OUT.h      generate a C header from Rust extern items
//! cargo tpt doc                          cargo doc for the workspace, warnings as errors
//! cargo tpt bench-gate [BASELINE] [RESULTS] [-- GATE_ARGS]
//!                                        run bench_suite and compare with a committed baseline
//! ```
//!
//! Exit codes: 0 when everything passed, 1 when a check failed, 2 on a usage error.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

const DEFAULT_BASELINE: &str = "crates/tpt-dev-bench/examples/ci/baseline.txt";
const DEFAULT_RESULTS: &str = "target/tpt-bench-results.txt";
const METADATA: &str = "target/tpt-dev-metadata.json";

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    // `cargo tpt ...` runs this binary as `cargo-tpt tpt ...`.
    if args.first().map(String::as_str) == Some("tpt") {
        args.remove(0);
    }
    let Some(command) = args.first().cloned() else {
        return usage();
    };
    let rest = &args[1..];
    let root = match workspace_root() {
        Some(r) => r,
        None => {
            eprintln!(
                "cargo tpt: run from inside the tpt-dev workspace (no [workspace] Cargo.toml found)"
            );
            return ExitCode::from(2);
        }
    };
    match command.as_str() {
        "check" => check(&root, rest),
        "lint" => lint(&root, rest),
        "dep" => dep(&root, rest),
        "headers" => headers(&root, rest),
        "doc" => doc(&root),
        "bench-gate" => bench_gate(&root, rest),
        "help" | "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("cargo tpt: unknown command `{other}`");
            usage()
        }
    }
}

fn usage() -> ExitCode {
    print_help();
    ExitCode::from(2)
}

fn print_help() {
    println!(
        "cargo tpt <command>\n\n\
         commands:\n  \
         check [--full]               run every project check (--full adds tests and cargo-deny)\n  \
         lint [--config FILE] [FILE]  run the project lint rules\n  \
         dep [ARGS]                   check dependency licences and cycles\n  \
         headers SRC GUARD OUT.h      generate a C header\n  \
         doc                          build docs with warnings as errors\n  \
         bench-gate [BASE] [RESULTS]  run the benchmark suite and the gate"
    );
}

/// Runs one step, prints a header, and returns whether it succeeded.
fn step(root: &Path, label: &str, program: &str, args: &[String], env: &[(&str, &str)]) -> bool {
    let shown = args.join(" ");
    let shown = if shown.chars().count() > 120 {
        let head: String = shown.chars().take(120).collect();
        format!("{head} ...")
    } else {
        shown
    };
    println!("\n==> {label}: {program} {shown}");
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(root).stdin(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    match cmd.status() {
        Ok(status) if status.success() => true,
        Ok(_) => {
            println!("    {label}: FAILED");
            false
        }
        Err(e) => {
            println!("    {label}: could not start {program}: {e}");
            false
        }
    }
}

/// Runs cargo with `args`.
fn cargo(root: &Path, label: &str, args: &[&str], env: &[(&str, &str)]) -> bool {
    let program = env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    step(root, label, &program, &strings(args), env)
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn check(root: &Path, args: &[String]) -> ExitCode {
    let full = match args {
        [] => false,
        [flag] if flag == "--full" => true,
        _ => return usage(),
    };

    let mut results: Vec<(&str, bool)> = vec![
        (
            "fmt",
            cargo(root, "fmt", &["fmt", "--all", "--", "--check"], &[]),
        ),
        (
            "clippy",
            cargo(
                root,
                "clippy",
                &["clippy", "--workspace", "--", "-D", "warnings"],
                &[],
            ),
        ),
        ("lint", lint_files(root, &[])),
        ("dep", dep_check(root, &[])),
    ];
    if full {
        results.push(("test", cargo(root, "test", &["test", "--workspace"], &[])));
        results.push(("cargo-deny", cargo_deny(root)));
    }

    println!("\nsummary:");
    let mut failed = false;
    for (name, ok) in &results {
        println!("  {name:<12} {}", if *ok { "ok" } else { "FAILED" });
        failed |= !ok;
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Runs `cargo deny check` when it is installed. A missing tool is reported,
/// not counted as a failure, so `check --full` still works without it.
fn cargo_deny(root: &Path) -> bool {
    let installed = Command::new(env::var("CARGO").unwrap_or_else(|_| String::from("cargo")))
        .args(["deny", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !installed {
        println!("\n==> cargo-deny: not installed, skipped (cargo install cargo-deny)");
        return true;
    }
    cargo(root, "cargo-deny", &["deny", "check"], &[])
}

fn lint(root: &Path, args: &[String]) -> ExitCode {
    let mut config = None;
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--config" {
            match it.next() {
                Some(path) => config = Some(path.clone()),
                None => return usage(),
            }
        } else {
            files.push(arg.clone());
        }
    }
    if lint_files_with(root, &files, config.as_deref()) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn lint_files(root: &Path, files: &[String]) -> bool {
    lint_files_with(root, files, None)
}

fn lint_files_with(root: &Path, files: &[String], config: Option<&str>) -> bool {
    let targets = if files.is_empty() {
        let mut found = Vec::new();
        collect_rust_files(&root.join("crates"), &mut found);
        found.sort();
        found
    } else {
        files.iter().map(PathBuf::from).collect()
    };
    if targets.is_empty() {
        println!("\n==> lint: no .rs files found");
        return false;
    }
    let mut args = vec![
        String::from("run"),
        String::from("-q"),
        String::from("-p"),
        String::from("tpt-dev-lint"),
        String::from("--"),
    ];
    if let Some(path) = config {
        args.push(String::from("--config"));
        args.push(path.to_string());
    }
    args.extend(targets.iter().map(|p| p.to_string_lossy().into_owned()));
    let program = env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    step(root, "lint", &program, &args, &[])
}

/// Every `.rs` file under `dir`, skipping build output and trybuild fixtures,
/// which are deliberately invalid code.
fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if matches!(name.as_ref(), "target" | "ui" | "fixtures" | ".git") {
                continue;
            }
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn dep(root: &Path, args: &[String]) -> ExitCode {
    if dep_check(root, args) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Reads cargo metadata, then runs `tpt-dev-dep` on it with the allow list from
/// `deny.toml`. Extra arguments are passed to `tpt-dev-dep`.
fn dep_check(root: &Path, extra: &[String]) -> bool {
    let metadata_path = root.join(METADATA);
    if let Some(parent) = metadata_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let cargo_bin = env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    let output = match Command::new(&cargo_bin)
        .args(["metadata", "--format-version", "1"])
        .current_dir(root)
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        Ok(o) => {
            eprintln!(
                "cargo metadata failed:\n{}",
                String::from_utf8_lossy(&o.stderr)
            );
            return false;
        }
        Err(e) => {
            eprintln!("could not run cargo metadata: {e}");
            return false;
        }
    };
    if let Err(e) = fs::write(&metadata_path, output) {
        eprintln!("cannot write {}: {e}", metadata_path.display());
        return false;
    }

    let mut args = vec![
        String::from("run"),
        String::from("-q"),
        String::from("-p"),
        String::from("tpt-dev-dep"),
        String::from("--"),
        String::from("--metadata"),
        METADATA.to_string(),
    ];
    for id in allowed_licenses(root) {
        args.push(String::from("--allow"));
        args.push(id);
    }
    args.extend(extra.iter().cloned());
    step(root, "dep", &cargo_bin, &args, &[])
}

/// The `allow` list from `deny.toml`, so the dependency check and cargo-deny
/// agree. Returns an empty list if the file cannot be read.
fn allowed_licenses(root: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join("deny.toml")) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    let mut in_allow = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !in_allow {
            if trimmed.starts_with("allow") && trimmed.contains('[') {
                in_allow = true;
                if trimmed.contains(']') {
                    break;
                }
            }
            continue;
        }
        if trimmed.starts_with(']') {
            break;
        }
        // A quoted identifier on this line, such as `"MIT",`.
        if let Some(start) = trimmed.find('"')
            && let Some(len) = trimmed[start + 1..].find('"')
        {
            ids.push(trimmed[start + 1..start + 1 + len].to_string());
        }
    }
    ids
}

fn headers(root: &Path, args: &[String]) -> ExitCode {
    if args.len() != 3 {
        return usage();
    }
    let mut full = vec![
        String::from("run"),
        String::from("-q"),
        String::from("-p"),
        String::from("tpt-dev-headers"),
        String::from("--"),
    ];
    full.extend(args.iter().cloned());
    let program = env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    if step(root, "headers", &program, &full, &[]) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn doc(root: &Path) -> ExitCode {
    if cargo(
        root,
        "doc",
        &["doc", "--workspace", "--no-deps", "--all-features"],
        &[("RUSTDOCFLAGS", "-D warnings")],
    ) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Runs `bench_suite` into the results file, then the gate against the baseline.
/// Arguments after `--` go to the gate.
fn bench_gate(root: &Path, args: &[String]) -> ExitCode {
    let (positional, gate_args): (&[String], &[String]) = match args.iter().position(|a| a == "--")
    {
        Some(i) => (&args[..i], &args[i + 1..]),
        None => (args, &[]),
    };
    let baseline = positional
        .first()
        .map_or(DEFAULT_BASELINE, String::as_str)
        .to_string();
    let results = positional
        .get(1)
        .map_or(DEFAULT_RESULTS, String::as_str)
        .to_string();
    if let Some(parent) = root.join(&results).parent() {
        let _ = fs::create_dir_all(parent);
    }

    let run_args: Vec<String> = [
        "run",
        "--release",
        "-q",
        "-p",
        "tpt-dev-bench",
        "--example",
        "bench_suite",
        "--features",
        "std",
        "--",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .chain(std::iter::once(results.clone()))
    .collect();
    let program = env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    if !step(root, "bench", &program, &run_args, &[]) {
        return ExitCode::from(1);
    }

    let mut gate: Vec<String> = [
        "run",
        "--release",
        "-q",
        "-p",
        "tpt-dev-bench",
        "--example",
        "gate",
        "--",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    gate.push(baseline);
    gate.push(results);
    gate.extend(gate_args.iter().cloned());
    if step(root, "gate", &program, &gate, &[]) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// The nearest ancestor of the current directory whose `Cargo.toml` declares a workspace.
fn workspace_root() -> Option<PathBuf> {
    let mut dir = env::current_dir().ok()?;
    loop {
        let manifest = dir.join("Cargo.toml");
        if fs::read_to_string(&manifest).is_ok_and(|t| t.contains("[workspace]")) {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_allow_list_from_deny_toml() {
        let dir = env::temp_dir().join(format!("tpt-cli-deny-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("deny.toml"),
            "[licenses]\nallow = [\n    \"MIT\",\n    \"ISC\", # comment\n]\n\n[bans]\n",
        )
        .unwrap();
        let ids = allowed_licenses(&dir);
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(ids, vec!["MIT".to_string(), "ISC".to_string()]);
    }

    #[test]
    fn skips_fixture_directories() {
        let dir = env::temp_dir().join(format!("tpt-cli-walk-{}", std::process::id()));
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("tests/ui")).unwrap();
        fs::write(dir.join("src/lib.rs"), "").unwrap();
        fs::write(dir.join("tests/ui/bad.rs"), "").unwrap();
        let mut found = Vec::new();
        collect_rust_files(&dir, &mut found);
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("lib.rs"));
    }
}

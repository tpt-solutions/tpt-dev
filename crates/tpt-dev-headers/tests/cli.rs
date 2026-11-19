//! Runs the built `tpt-dev-headers` binary and checks its output and exit codes.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_tpt-dev-headers");

/// A fresh temp directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("tpt-dev-headers-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

#[test]
fn writes_a_header_and_exits_zero() {
    let dir = scratch("ok");
    let src = dir.join("api.rs");
    let out = dir.join("api.h");
    std::fs::write(
        &src,
        "/// Adds two numbers.\n#[unsafe(no_mangle)]\npub extern \"C\" fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let result = run(&[src.to_str().unwrap(), "DEMO_H", out.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(0), "{result:?}");
    let header = std::fs::read_to_string(&out).unwrap();
    assert!(header.contains("#ifndef DEMO_H"));
    assert!(header.contains("int32_t add(int32_t a, int32_t b);"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unsupported_type_exits_one_with_a_message() {
    let dir = scratch("gen-error");
    let src = dir.join("bad.rs");
    let out = dir.join("bad.h");
    std::fs::write(
        &src,
        "#[unsafe(no_mangle)] pub extern \"C\" fn f(x: [u8; 4]) {}\n",
    )
    .unwrap();

    let result = run(&[src.to_str().unwrap(), "G", out.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("unsupported type"), "{stderr}");
    assert!(!out.exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_source_exits_one() {
    let dir = scratch("missing");
    let result = run(&[
        dir.join("nope.rs").to_str().unwrap(),
        "G",
        dir.join("x.h").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn wrong_argument_count_exits_two() {
    assert_eq!(run(&[]).status.code(), Some(2));
    assert_eq!(run(&["a.rs", "G"]).status.code(), Some(2));
    assert_eq!(run(&["a.rs", "G", "o.h", "extra"]).status.code(), Some(2));
}

#[test]
fn empty_guard_exits_two() {
    assert_eq!(run(&["a.rs", "", "o.h"]).status.code(), Some(2));
}

#[test]
fn help_exits_zero_and_prints_usage() {
    let result = run(&["--help"]);
    assert_eq!(result.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&result.stdout).contains("usage: tpt-dev-headers"));
}

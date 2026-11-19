//! `cargo metadata` parsing, checked against a real trimmed fixture and against
//! the `Cargo.lock` of the same packages, plus the command-line exit codes.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use tpt_dev_dep::{DepKind, Graph, Policy, Scope, analyse, parse_json, parse_lock, parse_metadata};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap()
}

fn policy() -> Policy {
    Policy {
        allow: vec!["MIT".into()],
    }
}

/// Each package's edges as sorted `name version` strings, for comparing graphs.
fn named_edges(g: &Graph, scope: Scope) -> Vec<(String, Vec<String>)> {
    let edges = g.edges(scope);
    let mut out: Vec<(String, Vec<String>)> = g
        .packages
        .iter()
        .zip(edges)
        .map(|(p, targets)| {
            let mut names: Vec<String> = targets
                .into_iter()
                .map(|i| format!("{} {}", g.packages[i].name, g.packages[i].version))
                .collect();
            names.sort();
            (format!("{} {}", p.name, p.version), names)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn fixture_has_the_resolved_packages_and_edge_kinds() {
    let g = parse_metadata(&fixture("cargo-metadata.json")).unwrap();
    assert_eq!(g.packages.len(), 9);

    let sim = g.packages.iter().find(|p| p.name == "tpt-dev-sim").unwrap();
    assert_eq!(sim.version, "0.0.0");
    assert_eq!(sim.source, None);
    assert_eq!(sim.dependencies.len(), 1);
    assert_eq!(sim.dependencies[0].kind, DepKind::Dev);
    assert_eq!(sim.dependencies[0].target, "tpt-dev-mem 0.0.0");

    let headers = g
        .packages
        .iter()
        .find(|p| p.name == "tpt-dev-headers")
        .unwrap();
    assert_eq!(headers.dependencies[0].kind, DepKind::Normal);
    assert_eq!(headers.dependencies[0].target, "syn 2.0.119");

    let syn = g.packages.iter().find(|p| p.name == "syn").unwrap();
    assert_eq!(
        syn.source.as_deref(),
        Some("registry+https://github.com/rust-lang/crates.io-index")
    );
}

#[test]
fn dev_edges_are_followed_only_when_asked() {
    let g = parse_metadata(&fixture("cargo-metadata.json")).unwrap();
    let sim = g
        .packages
        .iter()
        .position(|p| p.name == "tpt-dev-sim")
        .unwrap();
    assert!(g.edges(Scope::default())[sim].is_empty());
    assert_eq!(g.edges(Scope::ALL)[sim].len(), 1);
}

#[test]
fn metadata_with_all_edges_matches_the_lock_file_of_the_same_packages() {
    let meta = parse_metadata(&fixture("cargo-metadata.json")).unwrap();
    let lock = parse_lock(&fixture("Cargo.lock")).unwrap();

    // The lock file has no kinds, so every edge is normal: compare with ALL.
    assert_eq!(
        named_edges(&meta, Scope::ALL),
        named_edges(&lock, Scope::default())
    );

    let a = analyse(&meta, &[], &policy(), Scope::ALL);
    let b = analyse(&lock, &[], &policy(), Scope::default());
    assert_eq!(a.cycles, b.cycles);
    assert_eq!(a.transitive, b.transitive);
    assert!(a.cycles.is_empty());
}

#[test]
fn without_dev_edges_the_bloat_is_smaller_than_the_lock_file() {
    let meta = parse_metadata(&fixture("cargo-metadata.json")).unwrap();
    let r = analyse(&meta, &[], &policy(), Scope::default());
    let sim = r
        .transitive
        .iter()
        .find(|(n, _)| n == "tpt-dev-sim")
        .unwrap();
    assert_eq!(sim.1, 0);
    let headers = r
        .transitive
        .iter()
        .find(|(n, _)| n == "tpt-dev-headers")
        .unwrap();
    assert_eq!(
        headers.1, 4,
        "syn, proc-macro2, quote and unicode-ident are reachable"
    );
}

const DEV_CYCLE: &str = r#"{
  "packages": [
    {"name": "a", "version": "1.0.0", "id": "path+file:///ws/a#1.0.0", "source": null,
     "dependencies": [{"name": "b", "kind": null, "optional": false}]},
    {"name": "b", "version": "1.0.0", "id": "path+file:///ws/b#1.0.0", "source": null,
     "dependencies": [{"name": "a", "kind": "dev", "optional": false},
                      {"name": "log", "kind": null, "optional": true}]}
  ],
  "resolve": {"nodes": [
    {"id": "path+file:///ws/a#1.0.0", "deps": [
      {"name": "b", "pkg": "path+file:///ws/b#1.0.0", "dep_kinds": [{"kind": null, "target": null}]}
    ]},
    {"id": "path+file:///ws/b#1.0.0", "deps": [
      {"name": "a", "pkg": "path+file:///ws/a#1.0.0", "dep_kinds": [{"kind": "dev", "target": null}]}
    ]}
  ]}
}"#;

#[test]
fn a_dev_dependency_cycle_is_found_only_when_dev_edges_are_followed() {
    let g = parse_metadata(DEV_CYCLE).unwrap();
    let default = analyse(&g, &[], &policy(), Scope::default());
    assert!(default.cycles.is_empty());
    let all = analyse(&g, &[], &policy(), Scope::ALL);
    assert_eq!(all.cycles, vec![vec!["a".to_string(), "b".into()]]);
}

#[test]
fn non_optional_declared_edge_is_not_marked_optional() {
    let g = parse_metadata(DEV_CYCLE).unwrap();
    let b = g.packages.iter().find(|p| p.name == "b").unwrap();
    assert!(!b.dependencies[0].optional);
    // `log` is declared optional but has no resolve entry, so it is not an edge.
    assert_eq!(g.packages[1].dependencies.len(), 1);
}

#[test]
fn no_deps_output_uses_declared_dependencies_by_name() {
    let text = r#"{"packages": [
      {"name": "x", "version": "0.1.0", "source": null, "id": "path+file:///ws/x#0.1.0",
       "dependencies": [{"name": "y", "kind": "build", "optional": false},
                        {"name": "serde", "kind": null, "optional": true}]},
      {"name": "y", "version": "0.1.0", "source": null, "id": "path+file:///ws/y#0.1.0",
       "dependencies": []}
    ], "resolve": null}"#;
    let g = parse_metadata(text).unwrap();
    assert_eq!(g.packages[0].dependencies[0].kind, DepKind::Build);
    assert!(g.packages[0].dependencies[1].optional);
    assert!(g.edges(Scope::default())[0].is_empty());
    assert_eq!(
        g.edges(Scope {
            dev: false,
            build: true
        })[0],
        vec![1]
    );
}

#[test]
fn a_missing_resolve_key_is_treated_as_no_deps_output() {
    let text = r#"{"packages": [{"name": "x", "version": "1.0.0", "source": null,
      "id": "path+file:///x#1.0.0", "dependencies": []}]}"#;
    assert_eq!(parse_metadata(text).unwrap().packages.len(), 1);
}

#[test]
fn malformed_metadata_is_an_error_not_a_panic() {
    let bad: &[&str] = &[
        "",
        "{",
        "[]",
        r#"{"resolve": null}"#,
        r#"{"packages": "none"}"#,
        r#"{"packages": [{"name": "a", "version": "1", "dependencies": []}], "resolve": 3}"#,
        r#"{"packages": [{"name": "a", "version": "1", "source": 7, "dependencies": []}], "resolve": null}"#,
        r#"{"packages": [{"name": "a", "version": "1", "dependencies": [{"name": "b", "kind": "test"}]}], "resolve": null}"#,
        r#"{"packages": [{"name": "a", "version": "1", "dependencies": [{"name": "b", "optional": "yes"}]}], "resolve": null}"#,
        r#"{"packages": [{"name": "a\x", "version": "1", "dependencies": []}], "resolve": null}"#,
        r#"{"packages": [{"name": "\ud800", "version": "1", "dependencies": []}], "resolve": null}"#,
    ];
    for text in bad {
        assert!(parse_metadata(text).is_err(), "accepted {text:?}");
    }
}

#[test]
fn a_truncated_fixture_is_an_error() {
    let text = fixture("cargo-metadata.json");
    let cut = &text[..text.len() / 2];
    assert!(parse_metadata(cut).is_err());
}

#[test]
fn resolve_nodes_must_refer_to_known_packages() {
    let text = r#"{"packages": [{"name": "a", "version": "1.0.0", "id": "p#a@1.0.0",
      "dependencies": []}],
      "resolve": {"nodes": [{"id": "p#a@1.0.0", "deps": [
        {"name": "z", "pkg": "p#missing@1.0.0", "dep_kinds": [{"kind": null, "target": null}]}
      ]}]}}"#;
    let err = parse_metadata(text).unwrap_err();
    assert!(err.contains("not in packages"), "{err}");
}

#[test]
fn two_packages_with_one_name_and_version_are_rejected_with_a_resolve_graph() {
    let text = r#"{"packages": [
      {"name": "a", "version": "1.0.0", "id": "x#a@1.0.0", "dependencies": []},
      {"name": "a", "version": "1.0.0", "id": "y#a@1.0.0", "dependencies": []}
    ], "resolve": {"nodes": []}}"#;
    assert!(parse_metadata(text).is_err());
}

#[test]
fn deep_nesting_in_metadata_is_an_error() {
    let text = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
    assert!(parse_metadata(&text).is_err());
    assert!(parse_json(&text).is_err());
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tpt-dev-dep"))
}

fn fixture_path(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .display()
        .to_string()
}

#[test]
fn cli_reads_metadata_and_exits_zero_on_a_clean_graph() {
    let out = bin()
        .args([
            "--metadata",
            &fixture_path("cargo-metadata.json"),
            &fixture_path("licenses.txt"),
            "--allow",
            "MIT",
            "--allow",
            "Apache-2.0",
            "--allow",
            "Unicode-3.0",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ok: 9 packages"), "{stdout}");
}

#[test]
fn cli_exits_one_on_a_dev_cycle_only_with_include_dev() {
    let dir = std::env::temp_dir().join(format!("tpt-dev-dep-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("cycle.json");
    std::fs::write(&file, DEV_CYCLE).unwrap();
    let licenses = dir.join("licenses.txt");
    std::fs::write(
        &licenses,
        "a MIT
b MIT
",
    )
    .unwrap();
    let lic = licenses.display().to_string();
    let path = file.display().to_string();

    let clean = bin()
        .args(["--metadata", &path, &lic, "--allow", "MIT"])
        .output()
        .unwrap();
    assert_eq!(clean.status.code(), Some(0));

    let cyclic = bin()
        .args(["--metadata", &path, &lic, "--allow", "MIT", "--include-dev"])
        .output()
        .unwrap();
    assert_eq!(cyclic.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&cyclic.stdout).contains("cycle: a -> b"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cli_keeps_the_lock_file_path_and_its_exit_codes() {
    let lock = fixture_path("Cargo.lock");
    let out = bin()
        .args([&lock, &fixture_path("licenses.txt")])
        .args([
            "--allow",
            "MIT",
            "--allow",
            "Apache-2.0",
            "--allow",
            "Unicode-3.0",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("ok: 9 packages"));
}

#[test]
fn cli_usage_errors_exit_two() {
    let cases: &[&[&str]] = &[
        &[],
        &["--metadata"],
        &["--allow"],
        &["--bogus"],
        &["Cargo.lock", "a.txt", "extra"],
        &["--metadata", "x.json", "Cargo.lock"],
        &["/nonexistent/path/Cargo.lock"],
        &["--metadata", "/nonexistent/path/meta.json"],
    ];
    for args in cases {
        let out = bin().args(*args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "args {args:?}");
    }
}

#[test]
fn cli_malformed_metadata_exits_two() {
    let dir = std::env::temp_dir().join(format!("tpt-dev-dep-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("bad.json");
    std::fs::write(&file, r#"{"packages": [1,]}"#).unwrap();
    let out = bin()
        .args(["--metadata", &file.display().to_string()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

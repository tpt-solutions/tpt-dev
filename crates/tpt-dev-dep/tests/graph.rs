//! Graph analysis on a fabricated lock file, and on the workspace's real Cargo.lock.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_dep::{Policy, analyse, parse_lock};

const CYCLE3: &str = r#"
[[package]]
name = "a"
version = "1.0.0"
dependencies = [
 "b",
]

[[package]]
name = "b"
version = "1.0.0"
dependencies = ["c"]

[[package]]
name = "c"
version = "1.0.0"
dependencies = ["a", "d 2.0.0"]

[[package]]
name = "d"
version = "1.0.0"
dependencies = []

[[package]]
name = "d"
version = "2.0.0"
dependencies = []
"#;

fn policy() -> Policy {
    Policy {
        allow: vec!["MIT".into()],
    }
}

#[test]
fn finds_a_three_package_cycle_and_nothing_else() {
    let g = parse_lock(CYCLE3).unwrap();
    assert_eq!(g.packages.len(), 5);
    let r = analyse(&g, &[], &policy());
    assert_eq!(
        r.cycles,
        vec![vec!["a".to_string(), "b".into(), "c".into()]]
    );
}

#[test]
fn versioned_dependency_resolves_to_one_version() {
    let g = parse_lock(CYCLE3).unwrap();
    let c = g.resolve("c").unwrap();
    let d2 = g.resolve("d 2.0.0").unwrap();
    assert_eq!(g.packages[d2].version, "2.0.0");
    assert!(g.edges()[c].contains(&d2));
}

#[test]
fn missing_and_disallowed_licenses_are_violations() {
    let g = parse_lock(CYCLE3).unwrap();
    let licenses = vec![
        ("a".to_string(), "MIT".to_string()),
        ("b".to_string(), "GPL-3.0-only".to_string()),
    ];
    let r = analyse(&g, &licenses, &policy());
    assert!(
        r.license_violations
            .contains(&("b".into(), "GPL-3.0-only".into()))
    );
    assert!(
        r.license_violations
            .contains(&("c".into(), "missing".into()))
    );
    assert!(!r.is_clean());
}

#[test]
fn transitive_counts_follow_edges() {
    let g = parse_lock(CYCLE3).unwrap();
    let r = analyse(&g, &[], &policy());
    // From `a`, reaching b, c, a (itself, excluded) and d 2.0.0 gives 3.
    let a = r.transitive.iter().find(|(n, _)| n == "a").unwrap();
    assert_eq!(a.1, 3);
}

#[test]
fn the_workspace_lock_file_parses_and_has_no_cycles() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock");
    let Ok(text) = std::fs::read_to_string(path) else {
        // Cargo.lock is not committed in this workspace, so there is nothing to check.
        return;
    };
    let g = parse_lock(&text).unwrap();
    assert!(g.packages.len() > 10, "expected a populated lock file");
    let r = analyse(&g, &[], &policy());
    assert!(r.cycles.is_empty(), "unexpected cycles: {:?}", r.cycles);
}

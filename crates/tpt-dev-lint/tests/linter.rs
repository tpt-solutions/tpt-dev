//! The linter with config applied, end to end.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_lint::{Config, Linter, Severity, default_rules, has_errors};

const SAMPLE: &str = "fn read(p: *const u8) -> u8 {\n    let v: Option<u8> = None;\n    let _ = v.unwrap();\n    unsafe { *p }\n}\n";

#[test]
fn default_severities_report_and_fail_on_missing_safety() {
    let linter = Linter::new(default_rules());
    let diags = linter.lint_source(SAMPLE).unwrap();
    assert!(
        diags
            .iter()
            .any(|d| d.rule == "no-unwrap" && d.severity == Severity::Warn)
    );
    assert!(
        diags
            .iter()
            .any(|d| d.rule == "safety-comment" && d.severity == Severity::Deny)
    );
    assert!(has_errors(&diags));
}

#[test]
fn config_can_allow_a_rule() {
    let mut linter = Linter::new(default_rules());
    let config = Config::parse("no-unwrap = allow\nsafety-comment = warn\n").unwrap();
    linter.configure(&config).unwrap();
    let diags = linter.lint_source(SAMPLE).unwrap();
    assert!(diags.iter().all(|d| d.rule != "no-unwrap"));
    assert!(!has_errors(&diags));
}

#[test]
fn unknown_rule_in_config_is_an_error() {
    let mut linter = Linter::new(default_rules());
    let config = Config::parse("made-up-rule = deny\n").unwrap();
    assert!(linter.configure(&config).is_err());
}

#[test]
fn diagnostics_are_in_source_order_with_locations() {
    let linter = Linter::new(default_rules());
    let diags = linter.lint_source(SAMPLE).unwrap();
    let lines: Vec<usize> = diags.iter().map(|d| d.line).collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted);
    let unwrap = diags.iter().find(|d| d.rule == "no-unwrap").unwrap();
    assert_eq!((unwrap.line, unwrap.column), (3, 15));
}

#[test]
fn unparseable_source_is_reported() {
    let linter = Linter::new(default_rules());
    assert!(linter.lint_source("fn (").is_err());
}

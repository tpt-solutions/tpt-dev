//! Golden test: the generated header for the fixture is compared with a frozen copy.
//!
//! Set `BLESS=1` to rewrite the frozen copy after reviewing the change.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;

use tpt_dev_headers::{Options, generate};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn header_matches_the_golden_file() {
    let source = std::fs::read_to_string(fixture("api.rs")).unwrap();
    let got = generate(
        &source,
        &Options {
            guard: "TPT_SENSOR_H".into(),
        },
    )
    .unwrap();
    let golden = fixture("api.h");
    if std::env::var_os("BLESS").is_some() {
        std::fs::write(&golden, &got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&golden).expect("golden file missing; run with BLESS=1");
    assert_eq!(got, want, "generated header differs from the golden file");
}

#[test]
fn only_c_abi_items_appear() {
    let source = std::fs::read_to_string(fixture("api.rs")).unwrap();
    let got = generate(&source, &Options { guard: "G".into() }).unwrap();
    assert!(got.contains("session_open"));
    assert!(!got.contains("internal_helper"));
    assert!(got.contains("typedef struct Session Session;"));
}

#[test]
fn unsupported_types_are_errors_not_guesses() {
    let src = r#"
        #[unsafe(no_mangle)]
        pub extern "C" fn bad(x: [u8; 4]) -> u8 { x[0] }
    "#;
    let err = generate(src, &Options { guard: "G".into() }).unwrap_err();
    assert!(err.message.contains("unsupported type"));
}

#[test]
fn non_literal_discriminant_is_an_error() {
    let src = "#[repr(C)] pub enum E { A = 1 + 1 }";
    assert!(generate(src, &Options { guard: "G".into() }).is_err());
}

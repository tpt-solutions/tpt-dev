//! Malformed input: each validation rule is refused with its error, and no
//! truncated or corrupted fixture makes the parsers or the linker panic.

#![allow(clippy::unwrap_used)]

use tpt_dev_compose::module::ErrorKind;
use tpt_dev_compose::{Registry, parse, parse_component};

const HEADER: [u8; 8] = [0, b'a', b's', b'm', 1, 0, 0, 0];
const COMPONENT_HEADER: [u8; 8] = [0, b'a', b's', b'm', 0x0d, 0, 0x01, 0];

/// Encodes `n` as an unsigned LEB128 integer.
fn leb(mut n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let low = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(low);
            return out;
        }
        out.push(low | 0x80);
    }
}

/// A core module with the given (section id, body) pairs, in order.
fn module(sections: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = HEADER.to_vec();
    for (id, body) in sections {
        out.push(*id);
        out.extend(leb(body.len()));
        out.extend_from_slice(body);
    }
    out
}

fn kind_of(bytes: &[u8]) -> ErrorKind {
    parse(bytes).unwrap_err().kind
}

#[test]
fn a_table_export_with_no_table_is_refused() {
    // Export "t" of kind table, index 0.
    let m = module(&[(7, &[1, 1, b't', 1, 0])]);
    assert_eq!(kind_of(&m), ErrorKind::BadTableIndex(0));
}

#[test]
fn a_memory_export_with_no_memory_is_refused() {
    let m = module(&[(7, &[1, 1, b'm', 2, 0])]);
    assert_eq!(kind_of(&m), ErrorKind::BadMemoryIndex(0));
}

#[test]
fn a_global_export_with_no_global_is_refused() {
    let m = module(&[(7, &[1, 1, b'g', 3, 0])]);
    assert_eq!(kind_of(&m), ErrorKind::BadGlobalIndex(0));
}

#[test]
fn a_tag_export_with_no_tag_is_refused() {
    let m = module(&[(7, &[1, 1, b'e', 4, 0])]);
    assert_eq!(kind_of(&m), ErrorKind::BadTagIndex(0));
}

#[test]
fn an_export_index_past_the_defined_entries_is_refused() {
    // One memory, exported as index 1.
    let m = module(&[(5, &[1, 0, 1]), (7, &[1, 1, b'm', 2, 1])]);
    assert_eq!(kind_of(&m), ErrorKind::BadMemoryIndex(1));
}

#[test]
fn two_exports_with_one_name_are_refused() {
    let m = module(&[(5, &[1, 0, 1]), (7, &[2, 1, b'm', 2, 0, 1, b'm', 2, 0])]);
    assert_eq!(kind_of(&m), ErrorKind::DuplicateExport);
}

#[test]
fn a_memory_with_minimum_above_maximum_is_refused() {
    // flag 1, min 2, max 1.
    let m = module(&[(5, &[1, 1, 2, 1])]);
    assert_eq!(kind_of(&m), ErrorKind::LimitsOrder);
}

#[test]
fn a_memory_larger_than_the_address_space_is_refused() {
    // flag 0, min 65537 pages.
    let m = module(&[(5, &[1, 0, 0x81, 0x80, 0x04])]);
    assert_eq!(kind_of(&m), ErrorKind::MemoryTooLarge(65_537));
}

#[test]
fn a_global_with_a_bad_mutability_byte_is_refused() {
    // i32, mutability 2, init i32.const 0.
    let m = module(&[(6, &[1, 0x7f, 2, 0x41, 0, 0x0b])]);
    assert_eq!(kind_of(&m), ErrorKind::BadMutability(2));
}

#[test]
fn a_constant_expression_with_a_nop_is_refused() {
    let m = module(&[(6, &[1, 0x7f, 0, 0x01, 0x0b])]);
    assert_eq!(kind_of(&m), ErrorKind::BadConstOp(1));
}

#[test]
fn an_import_table_with_minimum_above_maximum_is_refused() {
    // Module "p", name "t", table of funcref with flag 1, min 2, max 1.
    let body = [1, 1, b'p', 1, b't', 1, 0x70, 1, 2, 1];
    assert_eq!(kind_of(&module(&[(2, &body)])), ErrorKind::LimitsOrder);
}

#[test]
fn an_exception_tag_import_must_have_attribute_zero() {
    let body = [1, 1, b'p', 1, b'e', 4, 1, 0];
    assert_eq!(kind_of(&module(&[(2, &body)])), ErrorKind::BadTagAttribute);
}

#[test]
fn a_tag_import_with_no_types_has_a_bad_type_index() {
    let body = [1, 1, b'p', 1, b'e', 4, 0, 0];
    assert_eq!(kind_of(&module(&[(2, &body)])), ErrorKind::BadTypeIndex(0));
}

#[test]
fn a_table_with_an_initial_value_parses_and_is_counted() {
    // Table section: 0x40 0x00, funcref, limits (flag 0, min 1), then ref.null func, end.
    let body = [1, 0x40, 0, 0x70, 0, 1, 0xd0, 0x70, 0x0b];
    let m = parse(&module(&[(4, &body)])).unwrap();
    assert_eq!(m.table_count(), 1);
    assert_eq!(m.table_type(0).unwrap().limits.min, 1);
}

#[test]
fn a_global_index_in_an_export_is_checked_against_imports_too() {
    // An imported global at index 0 makes index 0 valid, and index 1 is not.
    let body = [1, 1, b'p', 1, b'g', 3, 0x7f, 0];
    let ok = module(&[(2, &body), (7, &[1, 1, b'g', 3, 0])]);
    assert!(parse(&ok).is_ok());
    let bad = module(&[(2, &body), (7, &[1, 1, b'g', 3, 1])]);
    assert_eq!(kind_of(&bad), ErrorKind::BadGlobalIndex(1));
}

#[test]
fn a_component_with_a_truncated_type_section_is_refused() {
    // Type section of size 3 holding a record that claims two fields.
    let mut c = COMPONENT_HEADER.to_vec();
    c.extend_from_slice(&[7, 3, 1, 0x72, 2]);
    assert!(parse_component(&c).is_err());
}

#[test]
fn a_component_with_a_bad_value_type_is_refused() {
    // A list whose element is the negative type 0x70 (funcref), which is not a value type.
    let mut c = COMPONENT_HEADER.to_vec();
    c.extend_from_slice(&[7, 3, 1, 0x70, 0x70]);
    assert!(parse_component(&c).is_err());
}

/// Parses and links a set of modules, and checks that nothing panics.
fn link_all(bytes: &[&[u8]]) {
    let parsed: Vec<_> = bytes.iter().map(|b| parse(b)).collect();
    let mut registry = Registry::new();
    for (i, p) in parsed.iter().enumerate() {
        if let Ok(m) = p {
            let _ = registry.register(&format!("m{i}"), m);
        }
    }
    let _ = registry.link();
}

#[test]
fn every_truncation_of_the_fixtures_is_refused_without_a_panic() {
    let fixtures: [&[u8]; 6] = [
        include_bytes!("fixtures/provider.wasm"),
        include_bytes!("fixtures/importer.wasm"),
        include_bytes!("fixtures/math.wasm"),
        include_bytes!("fixtures/client.wasm"),
        include_bytes!("fixtures/component/nested.wasm"),
        include_bytes!("fixtures/component/types.wasm"),
    ];
    for f in fixtures {
        for n in 0..f.len() {
            let _ = parse(&f[..n]);
            let _ = parse_component(&f[..n]);
        }
    }
}

#[test]
fn every_single_byte_corruption_of_the_fixtures_is_handled_without_a_panic() {
    let fixtures: [&[u8]; 4] = [
        include_bytes!("fixtures/provider.wasm"),
        include_bytes!("fixtures/importer.wasm"),
        include_bytes!("fixtures/math.wasm"),
        include_bytes!("fixtures/component/types.wasm"),
    ];
    for f in fixtures {
        for i in 0..f.len() {
            for v in [0x00u8, 0x01, 0x40, 0x70, 0x7f, 0x80, 0xff] {
                let mut bytes = f.to_vec();
                bytes[i] = v;
                let _ = parse(&bytes);
                let _ = parse_component(&bytes);
            }
        }
    }
}

#[test]
fn corrupted_pairs_of_modules_do_not_panic_when_linked() {
    let provider = include_bytes!("fixtures/provider.wasm");
    let importer = include_bytes!("fixtures/importer.wasm");
    for i in 0..importer.len() {
        let mut bytes = importer.to_vec();
        bytes[i] ^= 0x5a;
        link_all(&[provider, &bytes]);
    }
}

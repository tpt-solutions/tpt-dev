//! Malformed component-level imports and exports. Each rule is refused with its
//! error, and truncated or corrupted component fixtures never panic, whether
//! they are parsed alone or linked.

#![allow(clippy::unwrap_used)]

use tpt_dev_compose::module::ErrorKind;
use tpt_dev_compose::{Registry, Space, parse_component as parse};

const COMPONENT_HEADER: [u8; 8] = [0, b'a', b's', b'm', 0x0d, 0, 0x01, 0];

const IMPORTS: &[u8] = include_bytes!("fixtures/component/imports.wasm");
const PROVIDER_LOG: &[u8] = include_bytes!("fixtures/component/provider_log.wasm");
const PROVIDER_BAD_LOG: &[u8] = include_bytes!("fixtures/component/provider_bad_log.wasm");
const EXPORT_INSTANCE: &[u8] = include_bytes!("fixtures/component/export_instance.wasm");

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

/// A component with the given (section id, body) pairs, in order.
fn component(sections: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = COMPONENT_HEADER.to_vec();
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
fn an_import_name_with_a_bad_flag_is_refused() {
    let c = component(&[(10, &[1, 0x01, 1, b'x', 0x01, 0])]);
    assert_eq!(kind_of(&c), ErrorKind::BadExternName(1));
}

#[test]
fn a_func_import_with_no_type_has_a_bad_index() {
    let c = component(&[(10, &[1, 0, 1, b'f', 0x01, 0])]);
    assert_eq!(kind_of(&c), ErrorKind::BadComponentIndex(Space::Type, 0));
}

#[test]
fn an_instance_import_with_no_type_has_a_bad_index() {
    let c = component(&[(10, &[1, 0, 1, b'i', 0x05, 0])]);
    assert_eq!(kind_of(&c), ErrorKind::BadComponentIndex(Space::Type, 0));
}

#[test]
fn an_unknown_import_descriptor_is_refused() {
    let c = component(&[(10, &[1, 0, 1, b'f', 0x09, 0])]);
    assert_eq!(kind_of(&c), ErrorKind::BadExternKind(9));
}

#[test]
fn a_core_import_that_is_not_a_module_is_refused() {
    let c = component(&[(10, &[1, 0, 1, b'f', 0x00, 0x00, 0])]);
    assert_eq!(kind_of(&c), ErrorKind::BadExternKind(0));
}

#[test]
fn a_bad_type_bound_is_refused() {
    let c = component(&[(10, &[1, 0, 1, b't', 0x03, 0x02])]);
    assert_eq!(kind_of(&c), ErrorKind::BadTypeBound(2));
}

#[test]
fn two_imports_with_one_name_are_refused() {
    let body = [2, 0, 1, b'x', 0x02, 0x7f, 0, 1, b'x', 0x02, 0x7f];
    let c = component(&[(10, &body)]);
    assert_eq!(kind_of(&c), ErrorKind::DuplicateImport);
}

#[test]
fn bytes_left_after_the_imports_are_refused() {
    let c = component(&[(10, &[1, 0, 1, b'x', 0x02, 0x7f, 0xff])]);
    assert_eq!(kind_of(&c), ErrorKind::TrailingBytes);
}

#[test]
fn a_valid_value_import_is_decoded() {
    let c = parse(&component(&[(10, &[1, 0, 1, b'x', 0x02, 0x7f])])).unwrap();
    assert_eq!(c.imports().count(), 1);
}

#[test]
fn an_export_with_an_unknown_sort_is_refused() {
    let c = component(&[(11, &[1, 0, 1, b'e', 0x09, 0, 0])]);
    assert_eq!(kind_of(&c), ErrorKind::BadSort(9));
}

#[test]
fn a_core_export_other_than_a_module_is_refused_as_unsupported() {
    let c = component(&[(11, &[1, 0, 1, b'e', 0x00, 0x00, 0, 0])]);
    assert_eq!(kind_of(&c), ErrorKind::UnsupportedSort(0));
}

#[test]
fn an_export_whose_descriptor_names_another_kind_is_refused() {
    // A func export with an instance descriptor.
    let c = component(&[(11, &[1, 0, 1, b'e', 0x01, 0, 1, 0x05, 0])]);
    assert_eq!(kind_of(&c), ErrorKind::DescKindMismatch);
}

#[test]
fn an_export_option_flag_other_than_zero_or_one_is_refused() {
    let c = component(&[(11, &[1, 0, 1, b'e', 0x03, 0, 0x02])]);
    assert_eq!(kind_of(&c), ErrorKind::BadOptionFlag(2));
}

#[test]
fn a_func_export_with_no_function_has_a_bad_index() {
    let c = component(&[(11, &[1, 0, 1, b'e', 0x01, 0, 0])]);
    assert_eq!(kind_of(&c), ErrorKind::BadComponentIndex(Space::Func, 0));
}

#[test]
fn a_core_module_export_with_no_module_has_a_bad_index() {
    let c = component(&[(11, &[1, 0, 1, b'm', 0x00, 0x11, 0, 0])]);
    assert_eq!(
        kind_of(&c),
        ErrorKind::BadComponentIndex(Space::CoreModule, 0)
    );
}

#[test]
fn two_exports_with_one_name_are_refused() {
    let types: &[u8] = &[1, 0x7f];
    let exports = [2, 0, 1, b'e', 0x03, 0, 0, 0, 1, b'e', 0x03, 0, 0];
    let c = component(&[(7, types), (11, &exports)]);
    assert_eq!(kind_of(&c), ErrorKind::DuplicateExport);
}

#[test]
fn a_type_export_of_an_existing_type_is_accepted() {
    let types: &[u8] = &[1, 0x7f];
    let exports = [1, 0, 1, b'e', 0x03, 0, 0];
    let c = parse(&component(&[(7, types), (11, &exports)])).unwrap();
    assert_eq!(c.exports().count(), 1);
}

#[test]
fn a_bad_index_inside_a_nested_component_is_refused() {
    // A nested component (section 4) whose own export is out of range.
    let inner = component(&[(11, &[1, 0, 1, b'e', 0x01, 0, 0])]);
    let outer = component(&[(4, &inner)]);
    assert_eq!(
        kind_of(&outer),
        ErrorKind::BadComponentIndex(Space::Func, 0)
    );
}

#[test]
fn every_truncation_of_the_component_fixtures_is_parsed_without_a_panic() {
    for f in [IMPORTS, PROVIDER_LOG, PROVIDER_BAD_LOG, EXPORT_INSTANCE] {
        for n in 0..f.len() {
            let _ = parse(&f[..n]);
        }
    }
}

#[test]
fn every_byte_corruption_of_the_component_fixtures_is_handled_without_a_panic() {
    for f in [IMPORTS, PROVIDER_LOG, PROVIDER_BAD_LOG, EXPORT_INSTANCE] {
        for i in 0..f.len() {
            for v in [0x00u8, 0x01, 0x05, 0x09, 0x11, 0x40, 0x7f, 0x80, 0xff] {
                let mut bytes = f.to_vec();
                bytes[i] = v;
                let _ = parse(&bytes);
            }
        }
    }
}

#[test]
fn corrupted_and_truncated_importers_do_not_panic_when_linked() {
    let provider = PROVIDER_LOG;
    for i in 0..IMPORTS.len() {
        for v in [0x00u8, 0x01, 0x05, 0x7f, 0xff] {
            let mut bytes = IMPORTS.to_vec();
            bytes[i] = v;
            link_pair(provider, &bytes);
        }
        link_pair(provider, &IMPORTS[..i]);
    }
}

/// Parses and links a provider and an importer as components, and checks that nothing panics.
fn link_pair(provider: &[u8], importer: &[u8]) {
    let (Ok(p), Ok(i)) = (parse(provider), parse(importer)) else {
        return;
    };
    let mut registry = Registry::new();
    let _ = registry.register_component("provider", &p);
    let _ = registry.register_component("importer", &i);
    let _ = registry.link();
}

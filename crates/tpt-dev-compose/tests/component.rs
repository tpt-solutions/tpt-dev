//! Conformance: component fixtures, checked against `wasm-tools dump`.
//!
//! The expected offsets and sizes below are the values `wasm-tools dump`
//! reports for the same files, taken from `tests/fixtures/component`.

#![allow(clippy::unwrap_used)]

use tpt_dev_compose::component::{Component, Section, parse};

const ADD: &[u8] = include_bytes!("fixtures/component/add.wasm");
const NESTED: &[u8] = include_bytes!("fixtures/component/nested.wasm");
const PLAIN: &[u8] = include_bytes!("fixtures/component/plain.wasm");

/// (id, offset, size, count) for each section, as `wasm-tools dump` lists them.
fn shape(c: &Component) -> Vec<(u8, usize, usize, Option<u32>)> {
    c.sections
        .iter()
        .map(|s: &Section| (s.id, s.offset, s.size, s.count))
        .collect()
}

#[test]
fn an_empty_component_matches_the_dump() {
    let c = parse(PLAIN).unwrap();
    assert!(c.sections.is_empty());
}

#[test]
fn add_has_the_sections_the_dump_lists() {
    let c = parse(ADD).unwrap();
    assert_eq!(
        shape(&c),
        [
            (1, 0x08, 0x34, None),
            (2, 0x3e, 4, Some(1)),
            (6, 0x44, 9, Some(1)),
            (7, 0x4f, 11, Some(1)),
            (8, 0x5c, 6, Some(1)),
            (11, 0x64, 9, Some(1)),
            (0, 0x6f, 0x3c, None),
        ]
    );
}

#[test]
fn add_embeds_a_core_module_that_exports_add() {
    let c = parse(ADD).unwrap();
    let module = c.sections[0].core_module.as_ref().unwrap();
    assert_eq!(module.func_count(), 1);
    assert_eq!(module.exports[0].name, "add");
    assert_eq!(c.sections[6].custom_name.as_deref(), Some("component-name"));
}

#[test]
fn nested_has_the_sections_the_dump_lists() {
    let c = parse(NESTED).unwrap();
    assert_eq!(
        shape(&c),
        [
            (0, 0x08, 10, None),
            (7, 0x14, 10, Some(1)),
            (10, 0x20, 13, Some(1)),
            (4, 0x2f, 0x55, None),
            (5, 0x86, 4, Some(1)),
            (0, 0x8c, 11, None),
            (0, 0x99, 0x25, None),
        ]
    );
    assert_eq!(c.sections[0].custom_name.as_deref(), Some("note"));
    assert_eq!(c.sections[5].custom_name.as_deref(), Some("note"));
}

#[test]
fn nested_contains_an_inner_component_with_a_core_module() {
    let c = parse(NESTED).unwrap();
    let inner = c.sections[3].nested.as_ref().unwrap();
    // The inner component's first section is its core module, at the offset
    // and size the dump reports.
    let first = &inner.sections[0];
    assert_eq!((first.id, first.offset, first.size), (1, 0x39, 42));
    let module = first.core_module.as_ref().unwrap();
    assert_eq!(module.exports[0].name, "f");
}

#[test]
fn section_kind_names_follow_the_specification() {
    assert_eq!(Section::kind_name(1), "core module");
    assert_eq!(Section::kind_name(11), "export");
    assert_eq!(Section::kind_name(13), "unknown");
}

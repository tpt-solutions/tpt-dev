//! Conformance: component fixtures, checked against `wasm-tools dump`.
//!
//! The expected offsets and sizes below are the values `wasm-tools dump`
//! reports for the same files, taken from `tests/fixtures/component`.

#![allow(clippy::unwrap_used)]

use tpt_dev_compose::component::{Component, Section, parse};
use tpt_dev_compose::types::{Primitive, TypeDef, ValueType};
use tpt_dev_compose::{ExternDesc, ExternKind, TypeBound};

const ADD: &[u8] = include_bytes!("fixtures/component/add.wasm");
const NESTED: &[u8] = include_bytes!("fixtures/component/nested.wasm");
const PLAIN: &[u8] = include_bytes!("fixtures/component/plain.wasm");
const IMPORTS: &[u8] = include_bytes!("fixtures/component/imports.wasm");
const PROVIDER_LOG: &[u8] = include_bytes!("fixtures/component/provider_log.wasm");
const EXPORT_INSTANCE: &[u8] = include_bytes!("fixtures/component/export_instance.wasm");

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

#[test]
fn imports_has_the_sections_the_dump_lists() {
    let c = parse(IMPORTS).unwrap();
    assert_eq!(
        shape(&c),
        [
            (7, 0x08, 8, Some(1)),
            (10, 0x12, 0x17, Some(2)),
            (0, 43, 22, None),
        ]
    );
}

#[test]
fn imports_lists_a_func_and_a_type_as_the_dump_does() {
    let c = parse(IMPORTS).unwrap();
    let imports: Vec<_> = c.imports().collect();
    // wasm-tools: ComponentImport { name: "host-log", ty: Func(0) }
    // and ComponentImport { name: "limit", ty: Type(Eq(0)) }.
    assert_eq!(imports.len(), 2);
    assert_eq!(imports[0].name, "host-log");
    assert_eq!(imports[0].desc, ExternDesc::Func(0));
    assert_eq!(imports[1].name, "limit");
    assert_eq!(imports[1].desc, ExternDesc::Type(TypeBound::Eq(0)));
    assert_eq!(imports[0].desc.kind(), ExternKind::Func);
    assert_eq!(imports[1].desc.kind(), ExternKind::Type);
}

#[test]
fn the_imported_func_type_is_decoded_from_the_type_section() {
    let c = parse(IMPORTS).unwrap();
    let f = c
        .type_def(0)
        .and_then(|d| match d {
            TypeDef::Func(f) => Some(f),
            _ => None,
        })
        .unwrap();
    assert_eq!(f.params[0].ty, ValueType::Primitive(Primitive::S32));
    assert_eq!(f.results[0].ty, ValueType::Primitive(Primitive::S32));
    // Type 1 is the type import, which is not decoded as a definition.
    assert!(c.type_def(1).is_none());
}

#[test]
fn provider_has_the_sections_the_dump_lists() {
    let c = parse(PROVIDER_LOG).unwrap();
    assert_eq!(
        shape(&c),
        [
            (7, 0x08, 9, Some(2)),
            (1, 0x13, 0x2e, None),
            (2, 0x43, 4, Some(1)),
            (6, 0x49, 7, Some(1)),
            (8, 0x52, 6, Some(1)),
            (11, 0x5a, 0x1a, Some(2)),
            (0, 118, 61, None),
        ]
    );
}

#[test]
fn provider_exports_a_func_with_a_declared_type_and_a_type() {
    let c = parse(PROVIDER_LOG).unwrap();
    let exports: Vec<_> = c.exports().collect();
    // wasm-tools: "host-log" kind Func index 0 with Some(Func(0)),
    // and "limit" kind Type index 1 with no descriptor.
    assert_eq!(exports.len(), 2);
    assert_eq!(exports[0].name, "host-log");
    assert_eq!(exports[0].kind, ExternKind::Func);
    assert_eq!(exports[0].index, 0);
    assert_eq!(exports[0].desc, Some(ExternDesc::Func(0)));
    assert_eq!(exports[1].name, "limit");
    assert_eq!(exports[1].kind, ExternKind::Type);
    assert_eq!(exports[1].index, 1);
    assert_eq!(exports[1].desc, None);
    assert!(matches!(
        c.type_def(1),
        Some(TypeDef::Primitive(Primitive::U32))
    ));
}

#[test]
fn an_instance_export_has_no_declared_type() {
    let c = parse(EXPORT_INSTANCE).unwrap();
    assert_eq!(
        shape(&c),
        [
            (1, 0x08, 0x34, None),
            (2, 0x3e, 4, Some(1)),
            (6, 0x44, 9, Some(1)),
            (7, 0x4f, 11, Some(1)),
            (8, 0x5c, 6, Some(1)),
            (5, 0x64, 10, Some(1)),
            (11, 0x70, 10, Some(1)),
            (0, 124, 70, None),
        ]
    );
    let exports: Vec<_> = c.exports().collect();
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "math");
    assert_eq!(exports[0].kind, ExternKind::Instance);
    assert_eq!(exports[0].index, 0);
    assert_eq!(exports[0].desc, None);
}

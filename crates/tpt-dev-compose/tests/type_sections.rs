//! Instance and component type sections, alias sections, and exact index bounds.
//! Fixtures are built by `wasm-tools` from the `.wat` sources next to them. The
//! hand-encoded bodies below follow the component binary format.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_compose::component::{CoreExportKind, OuterKind, parse};
use tpt_dev_compose::module::{ErrorKind, ValType};
use tpt_dev_compose::types::{Decl, TypeDef, ValueType};
use tpt_dev_compose::{Alias, ExternDesc, ExternKind, Space, TypeBound};

const TYPES_INSTANCE: &[u8] = include_bytes!("fixtures/component/types_instance.wasm");
const TYPES_ALIAS: &[u8] = include_bytes!("fixtures/component/types_alias.wasm");
const TYPES_RESOURCE: &[u8] = include_bytes!("fixtures/component/types_resource.wasm");
const ABI_SPILL_PARAMS: &[u8] = include_bytes!("fixtures/component/abi_spill_params.wasm");
const ABI_SPILL_RESULTS: &[u8] = include_bytes!("fixtures/component/abi_spill_results.wasm");
const LINK_PROVIDER_API: &[u8] = include_bytes!("fixtures/component/link_provider_api.wasm");

/// A component with the magic and version, then `body` as one section.
fn component_with(section_id: u8, body: &[u8]) -> Vec<u8> {
    let mut bytes = b"\0asm\x0d\0\x01\0".to_vec();
    bytes.push(section_id);
    assert!(body.len() < 128, "test bodies use one-byte sizes");
    bytes.push(body.len() as u8);
    bytes.extend_from_slice(body);
    bytes
}

/// The declarations of the first type in the first section, which must be an instance type.
fn instance_decls(c: &tpt_dev_compose::Component) -> Vec<Decl> {
    match &c.sections[0].types.as_ref().unwrap()[0] {
        TypeDef::Instance(decls) => decls.clone(),
        other => panic!("expected an instance type, got {other:?}"),
    }
}

#[test]
fn an_instance_type_decodes_its_declarations_in_order() {
    let c = parse(TYPES_INSTANCE).unwrap();
    let decls = instance_decls(&c);
    assert_eq!(decls.len(), 6);
    assert!(matches!(decls[0], Decl::Type(TypeDef::Record(_))));
    assert!(matches!(
        &decls[1],
        Decl::Export {
            name,
            desc: ExternDesc::Type(TypeBound::Eq(0)),
        } if name == "rec"
    ));
    // The type export "rec" defines local type 1, so the function is type 2.
    assert!(matches!(decls[2], Decl::Type(TypeDef::Func(_))));
    assert!(matches!(
        &decls[3],
        Decl::Export {
            name,
            desc: ExternDesc::Func(2),
        } if name == "f"
    ));
    assert!(matches!(decls[4], Decl::Type(TypeDef::Component(_))));
    assert!(matches!(
        &decls[5],
        Decl::Export {
            name,
            desc: ExternDesc::Component(_),
        } if name == "c"
    ));
}

#[test]
fn a_component_type_inside_an_instance_type_has_an_outer_alias() {
    let c = parse(TYPES_INSTANCE).unwrap();
    let decls = instance_decls(&c);
    let Decl::Type(TypeDef::Component(inner)) = &decls[4] else {
        panic!("expected a component type");
    };
    assert_eq!(inner.len(), 3);
    assert_eq!(
        inner[0],
        Decl::Alias(Alias::Outer {
            kind: OuterKind::Type,
            count: 1,
            index: 2,
        })
    );
    assert!(matches!(
        &inner[1],
        Decl::Import(i) if i.name == "x" && i.desc == ExternDesc::Func(0)
    ));
}

#[test]
fn an_instance_import_names_its_instance_type() {
    let c = parse(TYPES_INSTANCE).unwrap();
    let import = c.imports().next().unwrap();
    assert_eq!(import.name, "api");
    assert_eq!(import.desc, ExternDesc::Instance(0));
    assert!(matches!(c.type_def(0), Some(TypeDef::Instance(_))));
}

#[test]
fn the_alias_section_decodes_instance_and_core_aliases() {
    let c = parse(TYPES_ALIAS).unwrap();
    let aliases: Vec<&Alias> = c
        .sections
        .iter()
        .filter_map(|s| s.aliases.as_ref())
        .flatten()
        .collect();
    assert_eq!(aliases.len(), 2);
    assert_eq!(
        aliases[0],
        &Alias::InstanceExport {
            kind: ExternKind::Func,
            instance: 0,
            name: "f".into(),
        }
    );
    assert_eq!(
        aliases[1],
        &Alias::CoreInstanceExport {
            kind: CoreExportKind::Func,
            instance: 0,
            name: "k".into(),
        }
    );
    assert_eq!(aliases[0].space(), Some(Space::Func));
    assert_eq!(aliases[1].space(), Some(Space::CoreFunc));
}

#[test]
fn a_resource_with_a_destructor_and_a_sub_resource_import_decode() {
    let c = parse(TYPES_RESOURCE).unwrap();
    let types = c.sections.iter().find(|s| s.id == 7).unwrap();
    let TypeDef::Resource { rep, dtor } = &types.types.as_ref().unwrap()[0] else {
        panic!("expected a resource type");
    };
    assert_eq!(*rep, ValType::I32);
    assert!(dtor.is_some());
    let imports: Vec<_> = c.imports().collect();
    assert_eq!(imports[0].desc, ExternDesc::Type(TypeBound::SubResource));
    assert_eq!(imports[1].desc.kind(), ExternKind::Instance);
}

#[test]
fn a_component_type_declares_its_own_types_before_its_imports() {
    // Two types: func () -> (), then a component with a local func type and an
    // import of that local type.
    let body: &[u8] = &[
        0x02, // two types
        0x40, 0x00, 0x01, 0x00, // func () -> ()
        0x41, 0x02, // component with two declarations
        0x01, 0x40, 0x00, 0x01, 0x00, // type: func () -> ()
        0x03, 0x00, 0x01, b'x', 0x01, 0x00, // import "x": func of local type 0
    ];
    let c = parse(&component_with(7, body)).unwrap();
    let types = c.sections[0].types.as_ref().unwrap();
    let TypeDef::Component(decls) = &types[1] else {
        panic!("expected a component type");
    };
    assert_eq!(decls.len(), 2);
    assert!(matches!(&decls[1], Decl::Import(i) if i.name == "x"));
}

#[test]
fn a_component_type_import_may_not_name_an_outer_type() {
    // The same component type, but the import names type 0, which the
    // component's own declarations do not define.
    let body: &[u8] = &[
        0x02, 0x40, 0x00, 0x01, 0x00, // type 0 at the top level
        0x41, 0x01, 0x03, 0x00, 0x01, b'x', 0x01, 0x00,
    ];
    let e = parse(&component_with(7, body)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::BadComponentIndex(Space::Type, 0));
}

#[test]
fn a_reference_to_a_later_type_is_refused() {
    // A list of type 1, as the first type of the section: type 1 is not defined yet.
    let body: &[u8] = &[0x01, 0x70, 0x01];
    let e = parse(&component_with(7, body)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::BadComponentIndex(Space::Type, 1));
}

#[test]
fn an_instance_export_past_the_declared_types_is_refused() {
    // An instance type with one export: func, of type 3, which is not declared.
    let body: &[u8] = &[0x01, 0x42, 0x01, 0x04, 0x00, 0x01, b'f', 0x01, 0x03];
    let e = parse(&component_with(7, body)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::BadComponentIndex(Space::Type, 3));
}

#[test]
fn an_instance_type_refers_to_its_own_earlier_declarations() {
    // Inside an instance type: a record (type 0), then an export of it as type 0.
    let body: &[u8] = &[
        0x01, // one type
        0x42, 0x02, // instance with two declarations
        0x01, 0x72, 0x01, 0x01, b'a', 0x7f, // type: record { a: bool }
        0x04, 0x00, 0x01, b'r', 0x03, 0x00, 0x00, // export "r" as type bound eq 0
    ];
    let c = parse(&component_with(7, body)).unwrap();
    let decls = instance_decls(&c);
    assert!(matches!(
        &decls[1],
        Decl::Export {
            name,
            desc: ExternDesc::Type(TypeBound::Eq(0)),
        } if name == "r"
    ));
}

#[test]
fn an_instance_type_export_past_its_own_declarations_is_refused() {
    // An instance type with no declarations exports type 0, which it does not define.
    let body: &[u8] = &[0x01, 0x42, 0x01, 0x04, 0x00, 0x01, b'r', 0x03, 0x00, 0x00];
    let e = parse(&component_with(7, body)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::BadComponentIndex(Space::Type, 0));
}

#[test]
fn an_instance_type_may_not_import() {
    // Instance types have no import declarations, so 0x03 is a malformed tag there.
    let body: &[u8] = &[0x01, 0x42, 0x01, 0x03, 0x01, b'x', 0x01, 0x00];
    assert_eq!(
        parse(&component_with(7, body)).unwrap_err().kind,
        ErrorKind::BadTypeEncoding(0x03)
    );
}

#[test]
fn an_unknown_declaration_tag_is_refused() {
    let body: &[u8] = &[0x01, 0x42, 0x01, 0x05, 0x00];
    assert_eq!(
        parse(&component_with(7, body)).unwrap_err().kind,
        ErrorKind::BadTypeEncoding(0x05)
    );
}

#[test]
fn a_bad_resource_destructor_flag_is_refused() {
    let body: &[u8] = &[0x01, 0x3f, 0x7f, 0x02];
    assert_eq!(
        parse(&component_with(7, body)).unwrap_err().kind,
        ErrorKind::BadTypeEncoding(0x02)
    );
}

#[test]
fn a_resource_with_a_destructor_decodes() {
    let body: &[u8] = &[0x01, 0x3f, 0x7f, 0x01, 0x05];
    let c = parse(&component_with(7, body)).unwrap();
    assert_eq!(
        c.sections[0].types.as_ref().unwrap()[0],
        TypeDef::Resource {
            rep: ValType::I32,
            dtor: Some(5),
        }
    );
}

#[test]
fn a_module_type_leaves_the_section_undecoded_but_counted() {
    let body: &[u8] = &[0x01, 0x50, 0x00];
    let c = parse(&component_with(7, body)).unwrap();
    assert_eq!(c.sections[0].types, None);
    assert_eq!(c.sections[0].count, Some(1));
}

#[test]
fn an_async_function_type_is_left_undecoded() {
    let body: &[u8] = &[0x01, 0x43, 0x00, 0x00];
    let c = parse(&component_with(7, body)).unwrap();
    assert_eq!(c.sections[0].types, None);
}

#[test]
fn a_deeply_nested_instance_type_is_refused() {
    // Seventeen instance types, each declaring the next: past the depth limit of 16.
    let mut inner: Vec<u8> = vec![0x42, 0x00];
    for _ in 0..17 {
        let mut outer = vec![0x42, 0x01, 0x01];
        outer.extend_from_slice(&inner);
        inner = outer;
    }
    let mut body = vec![0x01];
    body.extend_from_slice(&inner);
    assert_eq!(
        parse(&component_with(7, &body)).unwrap_err().kind,
        ErrorKind::NestingTooDeep
    );
}

#[test]
fn a_nesting_within_the_limit_is_accepted() {
    let mut inner: Vec<u8> = vec![0x42, 0x00];
    for _ in 0..15 {
        let mut outer = vec![0x42, 0x01, 0x01];
        outer.extend_from_slice(&inner);
        inner = outer;
    }
    let mut body = vec![0x01];
    body.extend_from_slice(&inner);
    assert!(parse(&component_with(7, &body)).is_ok());
}

#[test]
fn an_unknown_alias_sort_is_refused() {
    // Alias section: one alias, instance export, sort byte 0x09.
    let body: &[u8] = &[0x01, 0x09, 0x00, 0x01, b'x'];
    assert_eq!(
        parse(&component_with(6, body)).unwrap_err().kind,
        ErrorKind::BadSort(0x09)
    );
}

#[test]
fn a_core_instance_alias_of_a_type_is_refused() {
    // Core sort 0x00 with kind byte 0x10 (a core type): not a core export kind an alias may take.
    let body: &[u8] = &[0x01, 0x00, 0x10, 0x01, 0x00, 0x01, b'x'];
    assert_eq!(
        parse(&component_with(6, body)).unwrap_err().kind,
        ErrorKind::BadSort(0x00)
    );
}

#[test]
fn an_outer_alias_adds_a_type_that_a_later_section_can_name() {
    // An outer type alias (sort 0x03, target 0x02, count 0, index 0) defines
    // type 0. The next type section item is list<0>, which is now defined.
    let mut bytes = b"\0asm\x0d\0\x01\0".to_vec();
    bytes.extend_from_slice(&[6, 5, 0x01, 0x03, 0x02, 0x00, 0x00]);
    bytes.extend_from_slice(&[7, 3, 0x01, 0x70, 0x00]);
    let c = parse(&bytes).unwrap();
    assert_eq!(
        c.sections[1].types.as_ref().unwrap()[0],
        TypeDef::List(ValueType::Index(0))
    );
}

#[test]
fn type_def_follows_the_type_index_space() {
    // The result fixture: type 0 is the tuple the result uses, type 1 the function.
    let c = parse(ABI_SPILL_RESULTS).unwrap();
    assert!(matches!(c.type_def(0), Some(TypeDef::Tuple(_))));
    assert!(matches!(c.type_def(1), Some(TypeDef::Func(_))));
    assert!(c.type_def(2).is_none());
    let spill = parse(ABI_SPILL_PARAMS).unwrap();
    assert!(matches!(spill.type_def(0), Some(TypeDef::Func(_))));
}

/// Every prefix of a fixture, and every single-byte corruption of it, must
/// parse or fail with an error. None may panic.
#[test]
fn truncated_and_corrupted_fixtures_do_not_panic() {
    let fixtures = [
        TYPES_INSTANCE,
        TYPES_ALIAS,
        TYPES_RESOURCE,
        ABI_SPILL_PARAMS,
        ABI_SPILL_RESULTS,
    ];
    for bytes in fixtures {
        for end in 0..=bytes.len() {
            let _ = parse(&bytes[..end]);
        }
        for at in 0..bytes.len() {
            for value in [0x00u8, 0x01, 0x03, 0x41, 0x42, 0x70, 0x7f, 0xff] {
                let mut copy = bytes.to_vec();
                copy[at] = value;
                let _ = parse(&copy);
            }
        }
    }
}

/// A section of nested instance and component types, with every byte in turn
/// replaced by every value, must never panic.
#[test]
fn every_byte_of_a_nested_type_section_may_be_replaced_without_a_panic() {
    let body: &[u8] = &[
        0x02, 0x72, 0x01, 0x01, b'a', 0x7f, 0x42, 0x03, 0x01, 0x01, 0x00, 0x04, 0x00, 0x01, b'r',
        0x03, 0x00, 0x00, 0x41, 0x02, 0x03, 0x00, 0x01, b'x', 0x01, 0x00,
    ];
    let bytes = component_with(7, body);
    for at in 8..bytes.len() {
        for value in 0u8..=255 {
            let mut copy = bytes.clone();
            copy[at] = value;
            let _ = parse(&copy);
        }
    }
}

#[test]
fn the_section_layout_matches_wasm_tools_dump() {
    // `wasm-tools dump types_alias.wasm`: the type section at 0x08 (size 0x15,
    // two types), the import section at 0x1f, and the alias section at 0x29, which
    // holds one alias.
    let c = parse(TYPES_ALIAS).unwrap();
    let shape: Vec<(u8, usize, usize, Option<u32>)> = c
        .sections
        .iter()
        .take(3)
        .map(|s| (s.id, s.offset, s.size, s.count))
        .collect();
    assert_eq!(
        shape,
        [
            (7, 0x08, 0x15, Some(2)),
            (10, 0x1f, 8, Some(1)),
            (6, 0x29, 6, Some(1)),
        ]
    );
}

#[test]
fn the_instance_type_layout_and_declarations_match_wasm_tools_dump() {
    // `wasm-tools dump types_instance.wasm`: one type section at 0x08, size 0x3d.
    // Its instance type's exports are "rec" = type eq 0, "f" = func 2 and
    // "c" = component 3, and the nested component's alias is outer 1, index 2.
    let c = parse(TYPES_INSTANCE).unwrap();
    assert_eq!(c.sections[0].id, 7);
    assert_eq!(c.sections[0].offset, 0x08);
    assert_eq!(c.sections[0].size, 0x3d);
    assert_eq!(c.sections[0].count, Some(1));
    let decls = instance_decls(&c);
    let exports: Vec<(&str, &ExternDesc)> = decls
        .iter()
        .filter_map(|d| match d {
            Decl::Export { name, desc } => Some((name.as_str(), desc)),
            _ => None,
        })
        .collect();
    assert_eq!(
        exports,
        [
            ("rec", &ExternDesc::Type(TypeBound::Eq(0))),
            ("f", &ExternDesc::Func(2)),
            ("c", &ExternDesc::Component(3)),
        ]
    );
}

#[test]
fn the_linking_provider_type_section_matches_wasm_tools_dump() {
    // `wasm-tools dump link_provider_api.wasm`: a type section at 0x08 of size
    // 0x11, one instance type whose export "f" is func 0.
    let c = parse(LINK_PROVIDER_API).unwrap();
    assert_eq!(
        (c.sections[0].id, c.sections[0].offset, c.sections[0].size),
        (7, 0x08, 0x11)
    );
    assert!(matches!(c.type_def(0), Some(TypeDef::Instance(_))));
}

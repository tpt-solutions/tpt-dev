//! Constructs beyond the basic fixtures: canonical functions, the value kinds
//! that are decoded, core module types, and outer aliases inside type
//! declarations.
//!
//! Each fixture is written in WAT under `tests/fixtures/constructs`, assembled
//! with `wasm-tools parse`, and accepted by `wasm-tools validate --features all`
//! (the fixed-length list, async and error-context types need that feature set).
//! The expected values are the ones `wasm-tools dump` prints for the same files.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_compose::component::{
    Alias, Canon, CanonOption, Component, ComponentImport, OuterKind, Section, parse,
};
use tpt_dev_compose::module::{ErrorKind, Import, ImportDesc, ValType};
use tpt_dev_compose::types::{
    AbiError, CoreType, Decl, Field, FuncType, Layout, ModuleDecl, Primitive, TypeDef, ValueType,
    flatten, layout,
};
use tpt_dev_compose::{ExternDesc, Space};

const CANON: &[u8] = include_bytes!("fixtures/constructs/canon.wasm");
const VALUE_KINDS: &[u8] = include_bytes!("fixtures/constructs/value_kinds.wasm");
const MODULE_TYPE: &[u8] = include_bytes!("fixtures/constructs/module_type.wasm");
const OUTER_ALIAS: &[u8] = include_bytes!("fixtures/constructs/outer_alias.wasm");

const HEADER: [u8; 8] = [0, b'a', b's', b'm', 0x0d, 0, 0x01, 0];

/// The canonical entries of a component, in section order.
fn canons(c: &Component) -> Vec<&Canon> {
    c.sections
        .iter()
        .filter_map(|s| s.canons.as_deref())
        .flatten()
        .collect()
}

/// The decoded types of the first type section that has any.
fn first_types(c: &Component) -> &[TypeDef] {
    c.sections.iter().find_map(|s| s.types.as_deref()).unwrap()
}

fn field(name: &str, ty: ValueType) -> Field {
    Field {
        name: String::from(name),
        ty,
    }
}

fn prim(p: Primitive) -> ValueType {
    ValueType::Primitive(p)
}

/// A component made of the given sections, each as (id, body). Bodies are short
/// enough that their sizes fit in one byte.
fn component_of(sections: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = HEADER.to_vec();
    for (id, body) in sections {
        out.push(*id);
        out.push(u8::try_from(body.len()).unwrap());
        out.extend_from_slice(body);
    }
    out
}

#[test]
fn canonical_functions_match_the_dump() {
    let c = parse(CANON).unwrap();
    let lift = Canon::Lift {
        core_func: 0,
        options: vec![
            CanonOption::Utf16,
            CanonOption::Memory(0),
            CanonOption::Realloc(1),
            CanonOption::PostReturn(2),
        ],
        type_index: 0,
    };
    let lower_h = Canon::Lower {
        func: 0,
        options: vec![
            CanonOption::Memory(0),
            CanonOption::Realloc(1),
            CanonOption::Utf8,
        ],
    };
    let lower_lifted = Canon::Lower {
        func: 1,
        options: vec![CanonOption::CompactUtf16],
    };
    assert_eq!(
        canons(&c),
        [
            &lift,
            &lower_h,
            &lower_lifted,
            &Canon::ResourceNew(1),
            &Canon::ResourceRep(1),
            &Canon::ResourceDrop(1),
        ]
    );
}

#[test]
fn section_offsets_match_the_dump() {
    let c = parse(CANON).unwrap();
    let shape: Vec<(u8, usize)> = c.sections.iter().map(|s| (s.id, s.offset)).collect();
    assert_eq!(
        shape,
        [
            (1, 0x08),
            (2, 0x6d),
            (6, 0x73),
            (7, 0x9c),
            (10, 0xa6),
            (8, 0xae),
            (11, 0xbd),
            (8, 0xcb),
            (7, 0xdc),
            (8, 0xe2),
            (0, 0xeb),
        ]
    );
}

#[test]
fn canonical_sections_count_their_entries() {
    let c = parse(CANON).unwrap();
    let counts: Vec<Option<u32>> = c
        .sections
        .iter()
        .filter(|s| s.id == 8)
        .map(|s: &Section| s.count)
        .collect();
    assert_eq!(counts, [Some(1), Some(2), Some(3)]);
}

#[test]
fn a_canonical_entry_may_name_only_what_is_already_defined() {
    // Type 0 is a function type with no parameters. Import 0 is `h`, a function
    // of that type, so component function 0 exists before the canonical section.
    let ty: &[u8] = &[0x01, 0x40, 0x00, 0x01, 0x00];
    let import: &[u8] = &[0x01, 0x00, 0x01, b'h', 0x01, 0x00];
    // One lower of component function 0. It defines core function 0.
    let ok: &[u8] = &[0x01, 0x01, 0x00, 0x00, 0x00];
    assert!(parse(&component_of(&[(7, ty), (10, import), (8, ok)])).is_ok());

    // A lower of component function 1, which does not exist.
    let missing_func: &[u8] = &[0x01, 0x01, 0x00, 0x01, 0x00];
    assert_eq!(
        parse(&component_of(&[(7, ty), (10, import), (8, missing_func)]))
            .unwrap_err()
            .kind,
        ErrorKind::BadComponentIndex(Space::Func, 1)
    );

    // After one lower defines core function 0, a lift of core function 1 is refused.
    // The bound is exact: the function space holds one entry, not an upper bound.
    let lift_past_end: &[u8] = &[
        0x02, 0x01, 0x00, 0x00, 0x00, // lower of component function 0
        0x00, 0x00, 0x01, 0x00, 0x00, // lift of core function 1, type 0
    ];
    assert_eq!(
        parse(&component_of(&[(7, ty), (10, import), (8, lift_past_end)]))
            .unwrap_err()
            .kind,
        ErrorKind::BadComponentIndex(Space::CoreFunc, 1)
    );

    // A lift with no core function defined before it.
    let lift_first: &[u8] = &[0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(
        parse(&component_of(&[(7, ty), (10, import), (8, lift_first)]))
            .unwrap_err()
            .kind,
        ErrorKind::BadComponentIndex(Space::CoreFunc, 0)
    );
}

#[test]
fn a_canonical_built_in_that_is_not_modelled_is_refused() {
    // Opcode 0x05 is `task.cancel`, which this parser does not model.
    let body: &[u8] = &[0x01, 0x05];
    assert_eq!(
        parse(&component_of(&[(8, body)])).unwrap_err().kind,
        ErrorKind::UnsupportedCanon(0x05)
    );
}

#[test]
fn a_canonical_option_that_changes_the_calling_convention_is_refused() {
    // A lower with the `async` option (0x06), which is not modelled.
    let body: &[u8] = &[0x01, 0x01, 0x00, 0x00, 0x01, 0x06];
    assert_eq!(
        parse(&component_of(&[(8, body)])).unwrap_err().kind,
        ErrorKind::UnsupportedCanonOption(0x06)
    );
}

#[test]
fn value_kinds_decode_as_the_dump_prints_them() {
    let c = parse(VALUE_KINDS).unwrap();
    let types = first_types(&c);
    let expected = [
        TypeDef::Stream(Some(prim(Primitive::U8))),
        TypeDef::Future(Some(prim(Primitive::String))),
        TypeDef::Map(prim(Primitive::String), prim(Primitive::U32)),
        TypeDef::FixedList(prim(Primitive::U8), 4),
        TypeDef::Primitive(Primitive::ErrorContext),
        TypeDef::AsyncFunc(FuncType {
            params: vec![field("x", prim(Primitive::U8))],
            results: vec![field("", prim(Primitive::U8))],
        }),
        TypeDef::Func(FuncType {
            params: vec![
                field("a", ValueType::Index(3)),
                field("b", ValueType::Index(4)),
            ],
            results: vec![field("", ValueType::Index(2))],
        }),
    ];
    assert_eq!(types, expected);
}

#[test]
fn fixed_lists_maps_and_handles_have_the_canonical_layouts() {
    let c = parse(VALUE_KINDS).unwrap();
    let space = first_types(&c);
    let u8_list4 = ValueType::Index(3);
    assert_eq!(layout(space, &u8_list4), Ok(Layout { size: 4, align: 1 }));
    assert_eq!(flatten(space, &u8_list4), Ok(vec![ValType::I32; 4]));
    // A map is a list of pairs: a pointer and a length.
    let map = ValueType::Index(2);
    assert_eq!(layout(space, &map), Ok(Layout { size: 8, align: 4 }));
    assert_eq!(flatten(space, &map), Ok(vec![ValType::I32, ValType::I32]));
    // Streams, futures and error-context values are i32 handles.
    for index in [0, 1, 4] {
        let ty = ValueType::Index(index);
        assert_eq!(layout(space, &ty), Ok(Layout { size: 4, align: 4 }));
        assert_eq!(flatten(space, &ty), Ok(vec![ValType::I32]));
    }
    // An async function type is not a value type.
    assert_eq!(
        layout(space, &ValueType::Index(5)),
        Err(AbiError::NotAValueType(5))
    );
}

#[test]
fn a_fixed_list_takes_its_elements_size_and_alignment() {
    // list<u32, 3> is 12 bytes with 4-byte alignment, and three i32 slots.
    let space = [TypeDef::FixedList(prim(Primitive::U32), 3)];
    assert_eq!(
        layout(&space, &ValueType::Index(0)),
        Ok(Layout { size: 12, align: 4 })
    );
    assert_eq!(
        flatten(&space, &ValueType::Index(0)),
        Ok(vec![ValType::I32; 3])
    );
    // list<string, 2> is two strings: 16 bytes, four i32 slots.
    let strings = [TypeDef::FixedList(prim(Primitive::String), 2)];
    assert_eq!(
        layout(&strings, &ValueType::Index(0)),
        Ok(Layout { size: 16, align: 4 })
    );
    assert_eq!(
        flatten(&strings, &ValueType::Index(0)),
        Ok(vec![ValType::I32; 4])
    );
    // A fixed list too large to flatten is refused rather than built.
    let huge = [TypeDef::FixedList(prim(Primitive::U8), u32::MAX)];
    assert_eq!(
        flatten(&huge, &ValueType::Index(0)),
        Err(AbiError::TooComplex)
    );
}

#[test]
fn an_empty_fixed_list_is_refused() {
    // A type section of one fixed list of bool with length 0.
    let body: &[u8] = &[0x01, 0x67, 0x7f, 0x00];
    assert_eq!(
        parse(&component_of(&[(7, body)])).unwrap_err().kind,
        ErrorKind::EmptyFixedList
    );
}

#[test]
fn a_module_type_declares_its_types_imports_exports_and_aliases() {
    let c = parse(MODULE_TYPE).unwrap();
    let TypeDef::Component(decls) = &first_types(&c)[0] else {
        panic!("the first type is a component type");
    };
    let Decl::CoreType(CoreType::Module(first)) = &decls[0] else {
        panic!("the first declaration is a module type");
    };
    assert_eq!(
        first,
        &vec![
            ModuleDecl::Type(module_func(&[ValType::I32], &[])),
            ModuleDecl::Import(Import {
                module: String::from("env"),
                name: String::from("f"),
                desc: ImportDesc::Func(0),
            }),
            ModuleDecl::Export {
                name: String::from("g"),
                desc: ImportDesc::Func(0),
            },
            ModuleDecl::Alias { count: 2, index: 0 },
            ModuleDecl::Export {
                name: String::from("h"),
                desc: ImportDesc::Func(1),
            },
        ]
    );

    let Decl::CoreType(CoreType::Module(second)) = &decls[1] else {
        panic!("the second declaration is a module type");
    };
    assert_eq!(
        second,
        &vec![
            ModuleDecl::Type(module_func(&[], &[])),
            ModuleDecl::Export {
                name: String::new(),
                desc: ImportDesc::Func(0),
            },
        ]
    );

    // The component imports a core module of the second module type.
    assert_eq!(
        decls[2],
        Decl::Import(ComponentImport {
            name: String::from("m"),
            desc: ExternDesc::CoreModule(1),
        })
    );
}

fn module_func(params: &[ValType], results: &[ValType]) -> tpt_dev_compose::module::FuncType {
    tpt_dev_compose::module::FuncType {
        params: params.to_vec(),
        results: results.to_vec(),
    }
}

#[test]
fn a_module_type_outer_alias_is_checked_against_its_scopes() {
    // The fixture's outer alias counts two scopes out. Changing the count to nine
    // names a scope that does not exist.
    let mut bytes = MODULE_TYPE.to_vec();
    // The declaration is 0x02, the type kind 0x10, the target 0x01, then count and index.
    let at = bytes
        .windows(5)
        .position(|w| w == [0x02, 0x10, 0x01, 0x02, 0x00])
        .unwrap();
    bytes[at + 3] = 0x09;
    assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::BadOuterScope(9));
}

#[test]
fn a_module_type_refers_only_to_its_own_types() {
    // A component type with a module type whose import names type 5, when the
    // module type has no types.
    let body: &[u8] = &[
        0x01, // one type
        0x41, 0x01, // component type with one declaration
        0x00, 0x50, // a core module type
        0x01, 0x00, // one module declaration: an import
        0x01, b'e', 0x01, b'f', // module "e", name "f"
        0x00, 0x05, // a function, of type 5
    ];
    assert_eq!(
        parse(&component_of(&[(7, body)])).unwrap_err().kind,
        ErrorKind::BadTypeIndex(5)
    );
}

#[test]
fn outer_aliases_in_component_types_resolve_to_their_scopes() {
    let c = parse(OUTER_ALIAS).unwrap();
    let nested = c.sections.iter().find_map(|s| s.nested.as_ref()).unwrap();
    // The nested component aliases the outer list as its type 0.
    let aliases: Vec<&Alias> = nested
        .sections
        .iter()
        .filter_map(|s| s.aliases.as_deref())
        .flatten()
        .collect();
    assert_eq!(
        aliases,
        [&Alias::Outer {
            kind: OuterKind::Type,
            count: 1,
            index: 0,
        }]
    );
    // Its component type $c reaches two scopes out for the list and one out for $m.
    let component_ty = nested
        .sections
        .iter()
        .find_map(|s| s.types.as_deref())
        .unwrap()
        .iter()
        .find_map(|d| match d {
            TypeDef::Component(decls) => Some(decls),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        component_ty[0],
        Decl::Alias(Alias::Outer {
            kind: OuterKind::Type,
            count: 2,
            index: 0,
        })
    );
    assert_eq!(
        component_ty[1],
        Decl::Alias(Alias::Outer {
            kind: OuterKind::Type,
            count: 1,
            index: 0,
        })
    );
}

/// Every prefix and every one-byte corruption of each construct fixture, and of
/// some older fixtures, must parse or fail with an error, and never panic.
#[test]
fn truncated_and_corrupted_constructs_never_panic() {
    const OLDER: &[&[u8]] = &[
        include_bytes!("fixtures/component/nested.wasm"),
        include_bytes!("fixtures/component/imports.wasm"),
        include_bytes!("fixtures/component/export_instance.wasm"),
    ];
    for bytes in [CANON, VALUE_KINDS, MODULE_TYPE, OUTER_ALIAS]
        .into_iter()
        .chain(OLDER.iter().copied())
    {
        for n in 0..bytes.len() {
            let _ = parse(&bytes[..n]);
        }
        let mut buf = bytes.to_vec();
        for i in 0..buf.len() {
            let original = buf[i];
            for v in 0..=255u8 {
                buf[i] = v;
                let _ = parse(&buf);
            }
            buf[i] = original;
        }
    }
}

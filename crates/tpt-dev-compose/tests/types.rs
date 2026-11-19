//! Conformance: the type section of a fixture assembled by `wasm-tools`.
//!
//! `fixtures/component/types.wat` declares one type of each defined kind, and
//! `wasm-tools` lists them in the same order. The expected values are those
//! listings. The layout checks use the canonical ABI rules worked out by hand.

#![allow(clippy::unwrap_used)]

use tpt_dev_compose::module::ValType;
use tpt_dev_compose::parse_component;
use tpt_dev_compose::types::{
    Case, Field, FuncType, Layout, Primitive, TypeDef, ValueType, flatten, layout,
};

const TYPES: &[u8] = include_bytes!("fixtures/component/types.wasm");

fn field(name: &str, ty: ValueType) -> Field {
    Field {
        name: name.into(),
        ty,
    }
}

fn prim(p: Primitive) -> ValueType {
    ValueType::Primitive(p)
}

fn decoded() -> Vec<TypeDef> {
    let c = parse_component(TYPES).unwrap();
    c.sections[0].types.clone().unwrap()
}

#[test]
fn the_type_section_decodes_to_the_types_wasm_tools_lists() {
    let expected = vec![
        TypeDef::Record(vec![
            field("a", prim(Primitive::U8)),
            field("b", prim(Primitive::U32)),
        ]),
        TypeDef::Variant(vec![
            Case {
                name: "x".into(),
                ty: None,
            },
            Case {
                name: "y".into(),
                ty: Some(prim(Primitive::U32)),
            },
        ]),
        TypeDef::List(prim(Primitive::String)),
        TypeDef::Option(prim(Primitive::U32)),
        TypeDef::Result {
            ok: Some(prim(Primitive::U32)),
            err: Some(prim(Primitive::U8)),
        },
        TypeDef::Result {
            ok: None,
            err: Some(prim(Primitive::U8)),
        },
        TypeDef::Tuple(vec![prim(Primitive::U8), prim(Primitive::U16)]),
        TypeDef::Flags(vec!["r".into(), "w".into()]),
        TypeDef::Enum(vec!["a".into(), "b".into()]),
        TypeDef::Func(FuncType {
            params: vec![field("p", prim(Primitive::U32))],
            results: vec![field("", prim(Primitive::U32))],
        }),
        TypeDef::Func(FuncType {
            params: vec![],
            results: vec![],
        }),
        TypeDef::Func(FuncType {
            params: vec![],
            results: vec![field("", prim(Primitive::U32))],
        }),
        TypeDef::Func(FuncType {
            params: vec![
                field("a", prim(Primitive::Bool)),
                field("b", prim(Primitive::Char)),
                field("c", prim(Primitive::String)),
            ],
            results: vec![],
        }),
    ];
    assert_eq!(decoded(), expected);
}

#[test]
fn the_fixture_types_have_the_canonical_abi_layouts() {
    let space = decoded();
    let at = |i: u32| layout(&space, &ValueType::Index(i)).unwrap();
    assert_eq!(at(0), Layout { size: 8, align: 4 }); // record {u8, u32}
    assert_eq!(at(1), Layout { size: 8, align: 4 }); // variant {x, y(u32)}
    assert_eq!(at(2), Layout { size: 8, align: 4 }); // list<string>
    assert_eq!(at(3), Layout { size: 8, align: 4 }); // option<u32>
    assert_eq!(at(4), Layout { size: 8, align: 4 }); // result<u32, u8>
    assert_eq!(at(5), Layout { size: 2, align: 1 }); // result<_, u8>
    assert_eq!(at(6), Layout { size: 4, align: 2 }); // tuple<u8, u16>
    assert_eq!(at(7), Layout { size: 1, align: 1 }); // flags of two
    assert_eq!(at(8), Layout { size: 1, align: 1 }); // enum of two
}

#[test]
fn the_fixture_types_flatten_as_the_canonical_abi_says() {
    let space = decoded();
    let flat = |i: u32| flatten(&space, &ValueType::Index(i)).unwrap();
    assert_eq!(flat(0), [ValType::I32, ValType::I32]);
    assert_eq!(flat(2), [ValType::I32, ValType::I32]);
    assert_eq!(flat(3), [ValType::I32, ValType::I32]);
    assert_eq!(flat(5), [ValType::I32, ValType::I32]);
    assert_eq!(flat(8), [ValType::I32]);
}

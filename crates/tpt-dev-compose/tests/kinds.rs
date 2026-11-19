//! Conformance: non-function imports, from fixtures assembled by `wasm-tools`.
//!
//! `provider.wasm` exports a table, a memory and two globals. `importer.wasm`
//! imports all four and links. `bad_importer.wasm` asks for the table with the
//! wrong element type.

#![allow(clippy::unwrap_used)]

use tpt_dev_compose::module::{ExportKind, ImportDesc, Limits, TableType, ValType};
use tpt_dev_compose::{LinkError, Registry, parse};

const PROVIDER: &[u8] = include_bytes!("fixtures/provider.wasm");
const IMPORTER: &[u8] = include_bytes!("fixtures/importer.wasm");
const BAD_IMPORTER: &[u8] = include_bytes!("fixtures/bad_importer.wasm");

#[test]
fn the_provider_defines_a_table_memory_and_globals_with_their_types() {
    let p = parse(PROVIDER).unwrap();
    assert_eq!(
        p.table_type(0),
        Some(TableType {
            elem: ValType::FuncRef,
            limits: Limits {
                min: 2,
                max: Some(4)
            },
        })
    );
    assert_eq!(p.memory_type(0), Some(Limits { min: 1, max: None }));
    assert!(!p.global_type(0).unwrap().mutable);
    assert!(p.global_type(1).unwrap().mutable);
    assert_eq!(p.global_count(), 2);
}

#[test]
fn the_importer_records_each_import_with_its_kind() {
    let i = parse(IMPORTER).unwrap();
    let kinds: Vec<ExportKind> = i.imports.iter().map(|x| x.desc.kind()).collect();
    assert_eq!(
        kinds,
        [
            ExportKind::Table,
            ExportKind::Memory,
            ExportKind::Global,
            ExportKind::Global
        ]
    );
    assert!(matches!(i.imports[1].desc, ImportDesc::Memory(_)));
}

#[test]
fn the_importer_links_to_the_provider_and_every_import_is_bound() {
    let p = parse(PROVIDER).unwrap();
    let i = parse(IMPORTER).unwrap();
    let mut registry = Registry::new();
    registry.register("p", &p).unwrap();
    registry.register("i", &i).unwrap();
    let plan = registry.link().unwrap();
    assert_eq!(plan.order, [0, 1]);
    assert_eq!(plan.bindings.len(), 4);
    for (k, b) in plan.bindings.iter().enumerate() {
        assert_eq!((b.importer, b.import, b.provider), (1, k, 0));
    }
    // The global "gm" is the second export of its kind, so it is found by name, not position.
    let gm = plan.bindings[3];
    assert_eq!(p.exports[gm.export].name, "gm");
}

#[test]
fn a_table_of_the_wrong_element_type_is_a_mismatch() {
    let p = parse(PROVIDER).unwrap();
    let bad = parse(BAD_IMPORTER).unwrap();
    let mut registry = Registry::new();
    registry.register("p", &p).unwrap();
    registry.register("i", &bad).unwrap();
    assert!(matches!(
        registry.link(),
        Err(LinkError::TypeMismatch {
            kind: ExportKind::Table,
            ..
        })
    ));
}

//! Conformance: fixtures assembled by `wasm-tools`, parsed and linked.
//!
//! The `.wasm` files are built from the `.wat` sources next to them with
//! `wasm-tools parse`. The tests check what the parser reads from them, and what
//! the linker does with them.

#![allow(clippy::unwrap_used)]

use tpt_dev_compose::module::{ExportKind, ImportDesc, ValType};
use tpt_dev_compose::{LinkError, Registry, parse};

const MATH: &[u8] = include_bytes!("fixtures/math.wasm");
const CLIENT: &[u8] = include_bytes!("fixtures/client.wasm");
const REEXPORT: &[u8] = include_bytes!("fixtures/reexport.wasm");
const BAD_CLIENT: &[u8] = include_bytes!("fixtures/bad_client.wasm");
const CYCLE_A: &[u8] = include_bytes!("fixtures/cycle_a.wasm");
const CYCLE_B: &[u8] = include_bytes!("fixtures/cycle_b.wasm");

#[test]
fn math_exports_its_functions_memory_and_has_no_imports() {
    let m = parse(MATH).unwrap();
    assert!(m.imports.is_empty());
    let names: Vec<(&str, ExportKind)> = m
        .exports
        .iter()
        .map(|e| (e.name.as_str(), e.kind))
        .collect();
    assert_eq!(
        names,
        [
            ("memory", ExportKind::Memory),
            ("add", ExportKind::Func),
            ("mul", ExportKind::Func)
        ]
    );
    let add = m.func_type(0).unwrap();
    assert_eq!(add.params, [ValType::I32, ValType::I32]);
    assert_eq!(add.results, [ValType::I32]);
}

#[test]
fn client_records_its_function_import() {
    let m = parse(CLIENT).unwrap();
    assert_eq!(m.imports.len(), 1);
    assert_eq!(m.imports[0].module, "math");
    assert_eq!(m.imports[0].name, "add");
    assert_eq!(m.imports[0].desc, ImportDesc::Func(0));
    // Type 0 is the imported signature; type 1 is the signature of "run".
    assert_eq!(m.functions, [1]);
    assert_eq!(m.start, None);
}

#[test]
fn a_reexported_import_has_the_imported_signature() {
    let m = parse(REEXPORT).unwrap();
    let times = m.exports.iter().find(|e| e.name == "times").unwrap();
    assert_eq!(times.kind, ExportKind::Func);
    let math = parse(MATH).unwrap();
    // "mul" is the second function math defines, so it has index 1.
    assert_eq!(m.func_type(times.index), math.func_type(1));
}

#[test]
fn client_links_to_math_add_and_math_comes_first() {
    let math = parse(MATH).unwrap();
    let client = parse(CLIENT).unwrap();
    let mut registry = Registry::new();
    registry.register("math", &math).unwrap();
    registry.register("client", &client).unwrap();
    let plan = registry.link().unwrap();

    assert_eq!(plan.order, [0, 1]);
    assert_eq!(plan.bindings.len(), 1);
    let b = plan.bindings[0];
    assert_eq!((b.importer, b.import, b.provider), (1, 0, 0));
    assert_eq!(math.exports[b.export].name, "add");
}

#[test]
fn a_signature_mismatch_is_reported() {
    let math = parse(MATH).unwrap();
    let bad = parse(BAD_CLIENT).unwrap();
    let mut registry = Registry::new();
    registry.register("math", &math).unwrap();
    registry.register("bad_client", &bad).unwrap();
    assert!(matches!(
        registry.link(),
        Err(LinkError::TypeMismatch { ref name, .. }) if name == "add"
    ));
}

#[test]
fn an_unregistered_module_is_reported() {
    let client = parse(CLIENT).unwrap();
    let mut alone = Registry::new();
    alone.register("client", &client).unwrap();
    assert!(matches!(alone.link(), Err(LinkError::UnknownModule { .. })));
}

#[test]
fn a_registered_module_without_the_export_is_reported() {
    // An empty module provides nothing, so it cannot satisfy the client's "add".
    // (A module that imports from itself would fail on its own imports first.)
    let client = parse(CLIENT).unwrap();
    let empty = parse(&[0, b'a', b's', b'm', 1, 0, 0, 0]).unwrap();
    let mut registry = Registry::new();
    registry.register("math", &empty).unwrap();
    registry.register("client", &client).unwrap();
    assert!(matches!(
        registry.link(),
        Err(LinkError::UnknownExport { ref name, .. }) if name == "add"
    ));
}

#[test]
fn mutually_dependent_modules_are_a_cycle() {
    let a = parse(CYCLE_A).unwrap();
    let b = parse(CYCLE_B).unwrap();
    let mut registry = Registry::new();
    registry.register("cycle_a", &a).unwrap();
    registry.register("cycle_b", &b).unwrap();
    // Each module imports from the other, so neither can be placed first.
    let stuck = match registry.link() {
        Err(LinkError::Cycle(names)) => names,
        _ => Vec::new(),
    };
    assert_eq!(stuck, ["cycle_a", "cycle_b"]);
}

#[test]
fn a_duplicate_name_is_rejected() {
    let math = parse(MATH).unwrap();
    let mut registry = Registry::new();
    registry.register("math", &math).unwrap();
    assert!(matches!(
        registry.register("math", &math),
        Err(LinkError::DuplicateName(_))
    ));
}

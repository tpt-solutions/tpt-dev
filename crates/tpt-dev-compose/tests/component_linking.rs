//! Component-level linking: imports resolved against registered components,
//! checked by name, kind and (where decodable) type, with the bindings recorded
//! in the plan. Fixtures are built by `wasm-tools` from the `.wat` sources next
//! to them.

#![allow(clippy::unwrap_used)]

use tpt_dev_compose::module::parse as parse_module;
use tpt_dev_compose::{
    ComponentBinding, ExternKind, LinkError, Registry, parse_component as parse,
};

const IMPORTS: &[u8] = include_bytes!("fixtures/component/imports.wasm");
const PROVIDER_LOG: &[u8] = include_bytes!("fixtures/component/provider_log.wasm");
const PROVIDER_BAD_LOG: &[u8] = include_bytes!("fixtures/component/provider_bad_log.wasm");
const MATH: &[u8] = include_bytes!("fixtures/math.wasm");

#[test]
fn component_imports_bind_to_the_provider_exports_by_name_and_kind() {
    let importer = parse(IMPORTS).unwrap();
    let provider = parse(PROVIDER_LOG).unwrap();
    let mut r = Registry::new();
    r.register_component("provider", &provider).unwrap();
    r.register_component("importer", &importer).unwrap();
    let plan = r.link().unwrap();
    assert_eq!(plan.component_names, ["provider", "importer"]);
    assert_eq!(plan.component_order, [0, 1]);
    assert_eq!(
        plan.component_bindings,
        [
            ComponentBinding {
                importer: 1,
                import: 0,
                provider: 0,
                export: 0,
                kind: ExternKind::Func,
            },
            ComponentBinding {
                importer: 1,
                import: 1,
                provider: 0,
                export: 1,
                kind: ExternKind::Type,
            },
        ]
    );
    // Modules are planned separately, so the module part of the plan is empty.
    assert!(plan.bindings.is_empty());
    assert!(plan.names.is_empty());
}

#[test]
fn a_component_import_with_no_provider_is_reported() {
    let importer = parse(IMPORTS).unwrap();
    let mut r = Registry::new();
    r.register_component("importer", &importer).unwrap();
    assert_eq!(
        r.link(),
        Err(LinkError::UnknownComponentExport {
            importer: "importer".into(),
            name: "host-log".into(),
            kind: ExternKind::Func,
        })
    );
}

#[test]
fn a_provider_with_a_different_func_type_is_a_type_mismatch() {
    let importer = parse(IMPORTS).unwrap();
    let bad = parse(PROVIDER_BAD_LOG).unwrap();
    let mut r = Registry::new();
    r.register_component("bad", &bad).unwrap();
    r.register_component("importer", &importer).unwrap();
    assert_eq!(
        r.link(),
        Err(LinkError::ComponentTypeMismatch {
            importer: "importer".into(),
            name: "host-log".into(),
            kind: ExternKind::Func,
        })
    );
}

#[test]
fn two_providers_of_one_name_and_kind_are_ambiguous() {
    let importer = parse(IMPORTS).unwrap();
    let good = parse(PROVIDER_LOG).unwrap();
    let bad = parse(PROVIDER_BAD_LOG).unwrap();
    let mut r = Registry::new();
    r.register_component("good", &good).unwrap();
    r.register_component("bad", &bad).unwrap();
    r.register_component("importer", &importer).unwrap();
    assert_eq!(
        r.link(),
        Err(LinkError::AmbiguousComponentExport {
            importer: "importer".into(),
            name: "host-log".into(),
            kind: ExternKind::Func,
        })
    );
}

#[test]
fn a_module_and_a_component_cannot_share_a_name() {
    let module = parse_module(MATH).unwrap();
    let component = parse(PROVIDER_LOG).unwrap();
    let mut r = Registry::new();
    r.register("shared", &module).unwrap();
    assert_eq!(
        r.register_component("shared", &component),
        Err(LinkError::DuplicateName("shared".into()))
    );
}

#[test]
fn core_and_component_bindings_are_planned_separately() {
    let module = parse_module(MATH).unwrap();
    let component = parse(IMPORTS).unwrap();
    let provider = parse(PROVIDER_LOG).unwrap();
    let mut r = Registry::new();
    r.register("math", &module).unwrap();
    r.register_component("provider", &provider).unwrap();
    r.register_component("importer", &component).unwrap();
    let plan = r.link().unwrap();
    assert_eq!(plan.names, ["math"]);
    assert_eq!(plan.order, [0]);
    assert_eq!(plan.component_bindings.len(), 2);
    assert!(
        plan.component_bindings
            .iter()
            .all(|b| b.kind != ExternKind::CoreModule)
    );
}

const LINK_PROVIDER_API: &[u8] = include_bytes!("fixtures/component/link_provider_api.wasm");
const LINK_PROVIDER_WIDE: &[u8] = include_bytes!("fixtures/component/link_provider_wide.wasm");
const LINK_IMPORTER_API: &[u8] = include_bytes!("fixtures/component/link_importer_api.wasm");
const LINK_IMPORTER_API_WIDE_ARG: &[u8] =
    include_bytes!("fixtures/component/link_importer_api_wide_arg.wasm");
const LINK_IMPORTER_API_MISSING: &[u8] =
    include_bytes!("fixtures/component/link_importer_api_missing.wasm");

#[test]
fn an_instance_import_links_to_a_provider_with_the_same_function_type() {
    let provider = parse(LINK_PROVIDER_API).unwrap();
    let importer = parse(LINK_IMPORTER_API).unwrap();
    let mut r = Registry::new();
    r.register_component("provider", &provider).unwrap();
    r.register_component("importer", &importer).unwrap();
    let plan = r.link().unwrap();
    assert_eq!(plan.component_bindings.len(), 1);
    assert_eq!(plan.component_bindings[0].kind, ExternKind::Instance);
}

#[test]
fn an_instance_provider_may_export_more_than_the_importer_asks_for() {
    let provider = parse(LINK_PROVIDER_WIDE).unwrap();
    let importer = parse(LINK_IMPORTER_API).unwrap();
    let mut r = Registry::new();
    r.register_component("provider", &provider).unwrap();
    r.register_component("importer", &importer).unwrap();
    assert!(r.link().is_ok());
}

#[test]
fn an_instance_export_with_a_different_parameter_type_is_a_mismatch() {
    let provider = parse(LINK_PROVIDER_API).unwrap();
    let importer = parse(LINK_IMPORTER_API_WIDE_ARG).unwrap();
    let mut r = Registry::new();
    r.register_component("provider", &provider).unwrap();
    r.register_component("importer", &importer).unwrap();
    assert_eq!(
        r.link(),
        Err(LinkError::ComponentTypeMismatch {
            importer: "importer".into(),
            name: "api".into(),
            kind: ExternKind::Instance,
        })
    );
}

#[test]
fn an_instance_import_of_an_export_the_provider_lacks_is_a_mismatch() {
    let provider = parse(LINK_PROVIDER_API).unwrap();
    let importer = parse(LINK_IMPORTER_API_MISSING).unwrap();
    let mut r = Registry::new();
    r.register_component("provider", &provider).unwrap();
    r.register_component("importer", &importer).unwrap();
    assert_eq!(
        r.link(),
        Err(LinkError::ComponentTypeMismatch {
            importer: "importer".into(),
            name: "api".into(),
            kind: ExternKind::Instance,
        })
    );
}

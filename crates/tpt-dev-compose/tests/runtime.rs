#![allow(clippy::unwrap_used)]
//! Instantiation and execution of linked modules, on the WAT fixtures in
//! `tests/fixtures/runtime` (assembled and validated with `wasm-tools`).

use tpt_dev_compose::module::ValType;
use tpt_dev_compose::{Feature, Registry, Runtime, RuntimeError, Trap, Value, parse};

const MATH: &[u8] = include_bytes!("fixtures/runtime/math.wasm");
const PROVIDER: &[u8] = include_bytes!("fixtures/runtime/provider.wasm");
const IMPORTER: &[u8] = include_bytes!("fixtures/runtime/importer.wasm");
const REEXPORT: &[u8] = include_bytes!("fixtures/runtime/reexport.wasm");
const MEMORY: &[u8] = include_bytes!("fixtures/runtime/memory.wasm");
const FLOAT: &[u8] = include_bytes!("fixtures/runtime/float.wasm");
const MUTPROV: &[u8] = include_bytes!("fixtures/runtime/mutprov.wasm");
const MUTABLE_IMPORT: &[u8] = include_bytes!("fixtures/runtime/mutable_import.wasm");
const EXTEND8: &[u8] = include_bytes!("fixtures/runtime/extend8.wasm");

fn runtime_of(modules: &[(&str, &[u8])]) -> Result<Runtime, RuntimeError> {
    let parsed: Vec<_> = modules
        .iter()
        .map(|(name, bytes)| (*name, parse(bytes).unwrap()))
        .collect();
    let mut registry = Registry::new();
    for (name, module) in &parsed {
        registry.register(name, module).unwrap();
    }
    let plan = registry.link().unwrap();
    Runtime::instantiate(&registry, &plan)
}

fn math() -> Runtime {
    runtime_of(&[("math", MATH)]).unwrap()
}

fn call(rt: &mut Runtime, export: &str, args: &[Value]) -> Vec<Value> {
    rt.invoke("math", export, args).unwrap()
}

fn i32s(values: &[i32]) -> Vec<Value> {
    values.iter().map(|&v| Value::I32(v)).collect()
}

#[test]
fn i32_arithmetic_and_comparisons() {
    let mut rt = math();
    assert_eq!(call(&mut rt, "add", &i32s(&[2, 40])), i32s(&[42]));
    assert_eq!(
        call(&mut rt, "add", &i32s(&[i32::MAX, 1])),
        i32s(&[i32::MIN])
    );
    assert_eq!(call(&mut rt, "div_s", &i32s(&[-7, 2])), i32s(&[-3]));
    assert_eq!(
        call(&mut rt, "div_u", &i32s(&[-1, 2])),
        i32s(&[0x7FFF_FFFF])
    );
    assert_eq!(call(&mut rt, "rem_s", &i32s(&[-7, 2])), i32s(&[-1]));
    assert_eq!(call(&mut rt, "shl", &i32s(&[1, 33])), i32s(&[2]));
    assert_eq!(
        call(&mut rt, "rotl", &i32s(&[0x8000_0001u32 as i32, 1])),
        i32s(&[3])
    );
    assert_eq!(call(&mut rt, "lt_u", &i32s(&[-1, 1])), i32s(&[0]));
    assert_eq!(call(&mut rt, "clz", &i32s(&[1])), i32s(&[31]));
    assert_eq!(call(&mut rt, "neg", &i32s(&[5])), i32s(&[-5]));
}

#[test]
fn integer_and_conversion_operations() {
    let mut rt = math();
    assert_eq!(
        rt.invoke("math", "i64ops", &[Value::I64(7), Value::I64(5)])
            .unwrap(),
        [Value::I64(2)]
    );
    assert_eq!(
        rt.invoke("math", "wrap", &[Value::I64(0x1_0000_0005)])
            .unwrap(),
        [Value::I32(5)]
    );
    assert_eq!(
        rt.invoke("math", "ext_u", &[Value::I32(-1)]).unwrap(),
        [Value::I64(4_294_967_295)]
    );
    assert_eq!(
        rt.invoke("math", "ext_s", &[Value::I32(-1)]).unwrap(),
        [Value::I64(-1)]
    );
}

#[test]
fn control_flow_loops_and_recursion() {
    let mut rt = math();
    assert_eq!(
        rt.invoke("math", "fact", &[Value::I64(10)]).unwrap(),
        [Value::I64(3_628_800)]
    );
    assert_eq!(call(&mut rt, "sum", &i32s(&[100])), i32s(&[5050]));
    assert_eq!(call(&mut rt, "sum", &i32s(&[0])), i32s(&[0]));
    assert_eq!(call(&mut rt, "pick", &i32s(&[1, 10, 20])), i32s(&[10]));
    assert_eq!(call(&mut rt, "pick", &i32s(&[0, 10, 20])), i32s(&[20]));
    assert_eq!(call(&mut rt, "sign", &i32s(&[-3])), i32s(&[-1]));
    assert_eq!(call(&mut rt, "sign", &i32s(&[0])), i32s(&[0]));
    assert_eq!(call(&mut rt, "sign", &i32s(&[8])), i32s(&[1]));
    assert_eq!(call(&mut rt, "tee", &i32s(&[7])), i32s(&[12]));
    assert_eq!(call(&mut rt, "early", &i32s(&[0])), i32s(&[7]));
    assert_eq!(call(&mut rt, "early", &i32s(&[1])), i32s(&[100]));
    assert_eq!(call(&mut rt, "block_result", &i32s(&[0])), i32s(&[3]));
    assert_eq!(call(&mut rt, "block_result", &i32s(&[1])), i32s(&[9]));
}

#[test]
fn globals_keep_their_value_between_calls() {
    let mut rt = math();
    assert_eq!(call(&mut rt, "bump", &[]), i32s(&[1]));
    assert_eq!(call(&mut rt, "bump", &[]), i32s(&[2]));
}

#[test]
fn traps_are_reported() {
    let mut rt = math();
    let trap = |rt: &mut Runtime, export: &str, args: &[Value]| {
        rt.invoke("math", export, args).unwrap_err()
    };
    assert_eq!(
        trap(&mut rt, "trap", &[]),
        RuntimeError::Trap(Trap::Unreachable)
    );
    assert_eq!(
        trap(&mut rt, "divz", &i32s(&[0])),
        RuntimeError::Trap(Trap::DivideByZero)
    );
    assert_eq!(
        trap(&mut rt, "div_s", &i32s(&[i32::MIN, -1])),
        RuntimeError::Trap(Trap::IntegerOverflow)
    );
    assert_eq!(
        trap(&mut rt, "deep", &i32s(&[0])),
        RuntimeError::Trap(Trap::CallDepthExceeded)
    );
    // Once the fuel is spent, the next call stops, and a larger budget runs again.
    rt.set_fuel(1_000);
    assert_eq!(
        trap(&mut rt, "spin", &[]),
        RuntimeError::Trap(Trap::OutOfFuel)
    );
    rt.set_fuel(1_000_000);
    assert_eq!(call(&mut rt, "add", &i32s(&[1, 1])), i32s(&[2]));
}

#[test]
fn invocation_errors_are_reported() {
    let mut rt = math();
    assert!(matches!(
        rt.invoke("nope", "add", &[]),
        Err(RuntimeError::UnknownInstance(_))
    ));
    assert!(matches!(
        rt.invoke("math", "missing", &[]),
        Err(RuntimeError::UnknownExport { .. })
    ));
    assert_eq!(
        rt.invoke("math", "add", &i32s(&[1])),
        Err(RuntimeError::ArgumentMismatch)
    );
    assert_eq!(
        rt.invoke("math", "add", &[Value::I32(1), Value::I64(2)]),
        Err(RuntimeError::ArgumentMismatch)
    );
}

#[test]
fn imports_bind_to_the_provider_and_the_start_function_runs() {
    let mut rt = runtime_of(&[("provider", PROVIDER), ("importer", IMPORTER)]).unwrap();
    // double(4) + base = 8 + 10
    assert_eq!(
        rt.invoke("importer", "f", &i32s(&[4])).unwrap(),
        i32s(&[18])
    );
    // The start function set the mutable global to 5, after it was initialised from base.
    assert_eq!(rt.invoke("importer", "seen", &[]).unwrap(), i32s(&[5]));
}

#[test]
fn a_provider_may_re_export_an_import() {
    let mut rt = runtime_of(&[("provider", PROVIDER), ("reexport", REEXPORT)]).unwrap();
    assert_eq!(
        rt.invoke("reexport", "twice_removed", &i32s(&[6])).unwrap(),
        i32s(&[12])
    );
}

#[test]
fn unsupported_features_are_refused_before_anything_runs() {
    let refused = |modules: &[(&str, &[u8])]| match runtime_of(modules) {
        Err(RuntimeError::Unsupported { feature, .. }) => Some(feature),
        _ => None,
    };
    assert_eq!(refused(&[("memory", MEMORY)]), Some(Feature::MemoryOrTable));
    assert_eq!(refused(&[("float", FLOAT)]), Some(Feature::NonIntegerType));
    assert_eq!(
        refused(&[("extend8", EXTEND8)]),
        Some(Feature::Opcode(0xC0))
    );
    assert_eq!(
        refused(&[("mutprov", MUTPROV), ("mutable_import", MUTABLE_IMPORT)]),
        Some(Feature::MutableGlobalImport)
    );
}

#[test]
fn value_types_are_the_two_integers() {
    assert_eq!(Value::I32(0).ty(), ValType::I32);
    assert_eq!(Value::I64(0).ty(), ValType::I64);
}

//! Canonical ABI: the flat-parameter spill and the 1-value result rule, and the
//! memory layout of spilled tuples. Expected values are worked out by hand from
//! the Component Model's canonical ABI. The fixtures are checked against
//! `wasm-tools validate`, which accepts each one and rejects the non-spilled
//! signature (see the comments in `fixtures/component/abi_*.wat`).

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_compose::component::parse;
use tpt_dev_compose::module::ValType;
use tpt_dev_compose::types::{
    Direction, Field, FuncType, Layout, MAX_FLAT_PARAMS, MAX_FLAT_RESULTS, Primitive, TypeDef,
    ValueType, func_abi, tuple_layout,
};

const SPILL_PARAMS: &[u8] = include_bytes!("fixtures/component/abi_spill_params.wasm");
const PARAMS_16: &[u8] = include_bytes!("fixtures/component/abi_params_16.wasm");
const SPILL_RESULTS: &[u8] = include_bytes!("fixtures/component/abi_spill_results.wasm");

const U8: ValueType = ValueType::Primitive(Primitive::U8);
const U16: ValueType = ValueType::Primitive(Primitive::U16);
const U32: ValueType = ValueType::Primitive(Primitive::U32);
const U64: ValueType = ValueType::Primitive(Primitive::U64);
const STRING: ValueType = ValueType::Primitive(Primitive::String);

fn unnamed(tys: &[ValueType]) -> Vec<Field> {
    tys.iter()
        .enumerate()
        .map(|(i, ty)| Field {
            name: format!("p{i}"),
            ty: *ty,
        })
        .collect()
}

fn func(params: &[ValueType], results: &[ValueType]) -> FuncType {
    FuncType {
        params: unnamed(params),
        results: unnamed(results),
    }
}

#[test]
fn the_limits_are_the_spec_values() {
    assert_eq!(MAX_FLAT_PARAMS, 16);
    assert_eq!(MAX_FLAT_RESULTS, 1);
}

#[test]
fn sixteen_flat_parameters_stay_in_registers() {
    let ft = func(&[U8; 16], &[]);
    let abi = func_abi(&[], &ft, Direction::Lower).unwrap();
    assert_eq!(abi.params, vec![ValType::I32; 16]);
    assert!(abi.param_memory.is_none());
    assert!(!abi.return_pointer);
}

#[test]
fn seventeen_flat_parameters_are_one_pointer_to_a_tuple() {
    let ft = func(&[U8; 17], &[]);
    let abi = func_abi(&[], &ft, Direction::Lift).unwrap();
    assert_eq!(abi.params, vec![ValType::I32]);
    let mem = abi.param_memory.unwrap();
    // Seventeen bytes, each at its own offset, aligned to one byte.
    assert_eq!(mem.layout, Layout { size: 17, align: 1 });
    assert_eq!(mem.offsets, (0..17).collect::<Vec<usize>>());
}

#[test]
fn a_spilled_tuple_pads_each_value_to_its_alignment() {
    // Seventeen flat values: u8 at 0, u32 at 4, u8 at 8, u16 at 10, then
    // thirteen u8 at 12 through 24. The tuple is 25 bytes, rounded to the
    // alignment 4 of the u32, so 28.
    let mut params = vec![U8, U32, U8, U16];
    params.extend([U8; 13]);
    let ft = func(&params, &[]);
    let abi = func_abi(&[], &ft, Direction::Lower).unwrap();
    assert_eq!(abi.params, vec![ValType::I32]);
    let mem = abi.param_memory.unwrap();
    assert_eq!(mem.layout, Layout { size: 28, align: 4 });
    let mut offsets = vec![0, 4, 8, 10];
    offsets.extend(12..25);
    assert_eq!(mem.offsets, offsets);
}

#[test]
fn seventeen_u64_parameters_take_136_bytes() {
    let ft = func(&[U64; 17], &[]);
    let abi = func_abi(&[], &ft, Direction::Lift).unwrap();
    let mem = abi.param_memory.unwrap();
    assert_eq!(
        mem.layout,
        Layout {
            size: 136,
            align: 8
        }
    );
    assert_eq!(mem.offsets[16], 128);
}

#[test]
fn a_string_counts_as_two_flat_values_toward_the_limit() {
    // Fifteen u8 plus one string is 17 flat values, so the parameters spill.
    let mut params = vec![U8; 15];
    params.push(STRING);
    let ft = func(&params, &[]);
    let abi = func_abi(&[], &ft, Direction::Lift).unwrap();
    assert_eq!(abi.params, vec![ValType::I32]);
    let mem = abi.param_memory.unwrap();
    // Fifteen bytes, then the string aligned to 4 at offset 16, size 24 rounded to 4.
    assert_eq!(mem.offsets[15], 16);
    assert_eq!(mem.layout, Layout { size: 24, align: 4 });
}

#[test]
fn a_single_u64_result_is_one_value_and_is_not_spilled() {
    let ft = func(&[], &[U64]);
    let abi = func_abi(&[], &ft, Direction::Lift).unwrap();
    assert_eq!(abi.results, vec![ValType::I64]);
    assert!(abi.result_memory.is_none());
}

#[test]
fn two_flat_results_are_spilled_and_lifted_as_a_pointer() {
    // A tuple of two u8 is two flat values, so the result goes through memory.
    let ft = func(&[U32], &[ValueType::Index(0)]);
    let space = [TypeDef::Tuple(vec![U8, U8])];
    let abi = func_abi(&space, &ft, Direction::Lift).unwrap();
    assert_eq!(abi.params, vec![ValType::I32]);
    assert_eq!(abi.results, vec![ValType::I32]);
    assert!(!abi.return_pointer);
    let mem = abi.result_memory.unwrap();
    assert_eq!(mem.layout, Layout { size: 2, align: 1 });
    assert_eq!(mem.offsets, vec![0]);
}

#[test]
fn a_lowered_spilled_result_takes_a_return_pointer_last() {
    let ft = func(&[U32], &[ValueType::Index(0)]);
    let space = [TypeDef::Tuple(vec![U8, U8])];
    let abi = func_abi(&space, &ft, Direction::Lower).unwrap();
    // The u32 parameter, then the return pointer.
    assert_eq!(abi.params, vec![ValType::I32, ValType::I32]);
    assert!(abi.results.is_empty());
    assert!(abi.return_pointer);
    assert!(abi.result_memory.is_some());
}

#[test]
fn sixteen_parameters_and_a_string_result_add_the_return_pointer_to_seventeen() {
    // The parameter count is checked before the return pointer is added, so
    // sixteen flat parameters are still in registers.
    let ft = func(&[U8; 16], &[STRING]);
    let abi = func_abi(&[], &ft, Direction::Lower).unwrap();
    assert_eq!(abi.params.len(), 17);
    assert_eq!(abi.params[16], ValType::I32);
    assert!(abi.param_memory.is_none());
    assert!(abi.return_pointer);
    let mem = abi.result_memory.unwrap();
    assert_eq!(mem.layout, Layout { size: 8, align: 4 });
}

#[test]
fn a_spilled_parameter_list_and_result_are_both_pointers() {
    let ft = func(&[U8; 17], &[STRING]);
    let abi = func_abi(&[], &ft, Direction::Lower).unwrap();
    assert_eq!(abi.params, vec![ValType::I32, ValType::I32]);
    assert!(abi.param_memory.is_some());
    assert!(abi.return_pointer);
}

#[test]
fn a_tuple_of_values_is_laid_out_in_order() {
    let space: [TypeDef; 0] = [];
    let mem = tuple_layout(&space, &[U8, U16, U8]).unwrap();
    assert_eq!(mem.offsets, vec![0, 2, 4]);
    assert_eq!(mem.layout, Layout { size: 6, align: 2 });
}

#[test]
fn the_fixture_with_seventeen_parameters_spills() {
    let c = parse(SPILL_PARAMS).unwrap();
    let import = c.imports().next().unwrap();
    let tpt_dev_compose::ExternDesc::Func(t) = import.desc else {
        panic!("expected a function import");
    };
    let TypeDef::Func(ft) = c.type_def(t).unwrap() else {
        panic!("expected a function type");
    };
    assert_eq!(ft.params.len(), 17);
    let abi = func_abi(&[], ft, Direction::Lower).unwrap();
    assert_eq!(abi.params, vec![ValType::I32]);
    let mem = abi.param_memory.unwrap();
    assert_eq!(mem.layout, Layout { size: 68, align: 4 });
    assert_eq!(mem.offsets[16], 64);
}

#[test]
fn the_fixture_with_sixteen_parameters_does_not_spill() {
    let c = parse(PARAMS_16).unwrap();
    let import = c.imports().next().unwrap();
    let tpt_dev_compose::ExternDesc::Func(t) = import.desc else {
        panic!("expected a function import");
    };
    let TypeDef::Func(ft) = c.type_def(t).unwrap() else {
        panic!("expected a function type");
    };
    let abi = func_abi(&[], ft, Direction::Lower).unwrap();
    assert_eq!(abi.params, vec![ValType::I32; 16]);
    assert!(abi.param_memory.is_none());
}

#[test]
fn the_fixture_with_a_tuple_result_takes_a_return_pointer() {
    let c = parse(SPILL_RESULTS).unwrap();
    let import = c.imports().next().unwrap();
    let tpt_dev_compose::ExternDesc::Func(t) = import.desc else {
        panic!("expected a function import");
    };
    // The result is the tuple at type 0, which the function type refers to.
    let TypeDef::Func(ft) = c.type_def(t).unwrap() else {
        panic!("expected a function type");
    };
    assert_eq!(ft.results[0].ty, ValueType::Index(0));
    assert_eq!(c.type_def(0), Some(&TypeDef::Tuple(vec![U32, U32])));
    let space = [TypeDef::Tuple(vec![U32, U32])];
    let lowered = func_abi(&space, ft, Direction::Lower).unwrap();
    assert_eq!(lowered.params, vec![ValType::I32, ValType::I32]);
    assert!(lowered.return_pointer);
    let lifted = func_abi(&space, ft, Direction::Lift).unwrap();
    assert_eq!(lifted.params, vec![ValType::I32]);
    assert_eq!(lifted.results, vec![ValType::I32]);
    assert_eq!(
        lifted.result_memory.unwrap().layout,
        Layout { size: 8, align: 4 }
    );
}

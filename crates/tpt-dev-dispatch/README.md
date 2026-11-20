# tpt-dev-dispatch

Heterogeneous compute task scheduler for CPU, NPU and GPU devices.

## Overview

`tpt-dev-dispatch` plans a graph of vector operations across several compute devices and runs the plan. A device is any type that implements the `Device` trait: it names its kind, says which operations it supports, estimates what each costs, and runs them. No vendor SDK is needed. The scheduler uses earliest-finish-time list scheduling, so each task goes to the device that would finish it soonest.

It is a small reference for the scheduling and device contract, for people who want to try placement policies or test code that targets several back ends. It ships a `CpuDevice` that runs everything and a `MockAccelerator` that supports only the operations it is given and records its calls. It has no dependencies on other `tpt-dev-*` crates.

## Features

- **Device trait:** `kind`, `supports`, `cost` and `run`. `Kind` is `Cpu`, `Npu` or `Gpu`; the scheduler uses it only for reporting.
- **Task graph:** `Graph` holds tasks that are vector operations (`Add`, `Mul`, `Relu`, `Sum`). A task takes the results of earlier tasks (`node`) or constant vectors (`constant`). A wrong number of inputs is rejected.
- **Cost model:** each device estimates ticks for an operation from the input size.
- **Scheduler:** `schedule` takes tasks in topological order and assigns each to the device with the earliest finish, given when the device is free and when the inputs are ready.
- **Execution:** `execute` runs a plan and returns each task's result vector, indexed by task ID.
- **Reference devices:** `CpuDevice` and `MockAccelerator`. Mixed plans give the same results as CPU-only plans.

## Installation

```sh
cargo add tpt-dev-dispatch
```

The crate is `#![no_std]` but needs `alloc`: graphs, plans and results are `Vec`s. It is not published yet; until then, depend on it by path or git.

| Feature | Enables | Default |
|---|---|---|
| `std` | links `std` (used for tests and host tooling) | off |

The CI builds the workspace for `thumbv7em-none-eabihf`, which includes this crate.

## Quick start

```rust
use tpt_dev_dispatch::{CpuDevice, Device, Graph, MockAccelerator, Op, execute, schedule};

let mut g = Graph::new();
let x = g.constant(Op::Add, vec![vec![1.0, -2.0], vec![3.0, 4.0]]);
let y = g.node(Op::Relu, &[x]);
let _ = g.node(Op::Sum, &[y]);

let mut devices: Vec<Box<dyn Device>> = vec![
    Box::new(CpuDevice::default()),
    Box::new(MockAccelerator::new(&[Op::Add])),
];
let plan = schedule(&g, &devices).unwrap();
let results = execute(&g, &plan, &mut devices).unwrap();
assert_eq!(results[2], [6.0]);
```

Each `Assignment` in `plan.assignments` gives the task, the device index, and the start and finish ticks. `plan.makespan` is the tick at which the last task finishes.

## Design notes and guarantees

- **Cost model.** `CpuDevice` costs `50 + rate * elements` ticks, where `rate` is `per_element` (1 when left at zero). `MockAccelerator` costs `200 + elements / 4`, so it has a larger start-up cost and wins on large inputs.
- **Placement.** Ties on finish time go to the lower device index. A device is considered only if `supports` is true for the operation.
- **Validation.** `Graph::order` checks arity and input references and returns a topological order. `GraphError` reports `Arity`, `BadInput` and `Cycle`. The graph API only lets a task refer to earlier tasks, so a cycle cannot be built through it; the check is a guard.
- **Errors.** `ScheduleError` is `Graph`, `NoDevice` (no device supports a task's operation) or `Refused` (a device returned `None` at run time).
- **Plans are deterministic.** The same graph and devices give the same plan.

## Limitations and scope

- **Estimates, not measurements.** A schedule is only as good as the cost model. The mock accelerator's numbers are invented to show the trade-off, not taken from hardware.
- **Heuristic, not optimal.** Earliest-finish-time list scheduling is greedy, so a plan can be worse than the best one.
- **No data transfer cost.** The model ignores the time to move results between devices.
- **Vector operations only.** There is no matrix multiply, no convolution and no tensor shapes.
- **Sequential execution.** `execute` runs tasks one after another in plan order. It does not run them in parallel, even where the plan overlaps them in time.
- **Heap allocation.** The crate uses `alloc`, so it needs an allocator.
- **No GPU or NPU back end.** `Kind::Gpu` and `Kind::Npu` exist for reporting. The only shipped devices are `CpuDevice` and `MockAccelerator` (which reports `Npu`).
- **Constant tasks only at the leaves.** A task has either constant inputs or task inputs, not a mix.

## Related crates

This crate does not depend on other `tpt-dev-*` crates and none of them depend on it. For other runtime pieces, see [`tpt-dev-async`](https://docs.rs/tpt-dev-async) (async executor) and [`tpt-dev-sim`](https://docs.rs/tpt-dev-sim) (cycle-accurate simulator).

## Status and MSRV

Pre-release (0.0.0, not yet published). The minimum supported Rust version is 1.98 (`rust-version` in `Cargo.toml`).

## License

Licensed under either of MIT or Apache-2.0, at your option.

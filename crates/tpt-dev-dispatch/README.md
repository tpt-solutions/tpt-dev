# tpt-dev-dispatch

A scheduler for heterogeneous compute. It plans a task graph across CPU, NPU and GPU devices, using earliest-finish-time list scheduling, and runs the plan. No vendor SDK is needed: a device is any type that implements `Device`.

- **Device trait:** a device names its kind, says which operations it supports, estimates the cost of each, and runs them. The `CpuDevice` reference runs everything. `MockAccelerator` supports only the operations it is given, and records its calls.
- **Task graph:** tasks are vector operations (`Add`, `Mul`, `Relu`, `Sum`). A task takes the results of earlier tasks or constant inputs. Operations with the wrong number of inputs are rejected.
- **Cost model:** each device estimates the ticks an operation takes, from the size of its inputs. Costs are estimates for planning, not measurements.
- **Scheduler:** tasks are taken in topological order. Each goes to the device that finishes it soonest, given when the device is free and when the inputs are ready.
- **Execution:** `execute` runs the plan and returns each task's result. Mixed plans give the same results as CPU-only plans.

## Example

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

## Limits

- **Estimates, not measurements.** The schedule is only as good as the cost model. The mock accelerator's numbers are invented to show the trade-off, not taken from hardware.
- **Heuristic, not optimal.** Earliest-finish-time list scheduling is a greedy method, so a plan can be worse than the best one.
- **No data transfer cost.** The model ignores the time to move results between devices.
- **Vector operations only.** There is no matrix multiply, no convolution, and no tensor shapes. The operations are enough to show the scheduling and the device contract.
- **Sequential execution.** `execute` runs tasks one after another, in plan order. It does not run them in parallel.
- **Vectors are heap-allocated.** The crate uses `alloc`, so it needs an allocator.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

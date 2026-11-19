//! Earliest-finish-time list scheduling, and execution of a schedule.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::device::{Device, Op};
use crate::graph::{Graph, GraphError, Task, TaskId};

/// Where and when one task runs, in the device model's ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Assignment {
    /// The task.
    pub task: TaskId,
    /// Index of the device in the list given to [`schedule`].
    pub device: usize,
    /// Tick the task starts.
    pub start: u64,
    /// Tick the task finishes.
    pub finish: u64,
}

/// A complete plan for a graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    /// One assignment per task, in topological order.
    pub assignments: Vec<Assignment>,
    /// The tick at which the last task finishes.
    pub makespan: u64,
}

/// Why a graph could not be scheduled or executed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    /// The graph itself is invalid.
    Graph(GraphError),
    /// No device supports the task's operation.
    NoDevice(TaskId),
    /// The assigned device refused the operation at run time.
    Refused(TaskId),
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScheduleError::Graph(e) => write!(f, "{e}"),
            ScheduleError::NoDevice(t) => write!(f, "no device can run task {}", t.0),
            ScheduleError::Refused(t) => write!(f, "device refused task {}", t.0),
        }
    }
}

/// The number of elements a task produces, given the sizes of its inputs.
fn output_size(task: &Task, sizes: &[usize]) -> usize {
    match &task.constant {
        Some(data) => data.iter().map(Vec::len).max().unwrap_or(0),
        None => match task.op {
            Op::Sum => 1,
            _ => task.inputs.first().map_or(0, |t| sizes[t.0]),
        },
    }
}

/// Plans where each task runs, using earliest-finish-time list scheduling.
///
/// Tasks are taken in topological order. Each goes to the device that finishes
/// it soonest, given the device's cost estimate, the time it becomes free, and
/// the time its inputs are ready. Ties go to the lower device index.
pub fn schedule(graph: &Graph, devices: &[Box<dyn Device>]) -> Result<Schedule, ScheduleError> {
    let order = graph.order().map_err(ScheduleError::Graph)?;
    let n = graph.tasks.len();
    let mut sizes = vec![0usize; n];
    let mut finish = vec![0u64; n];
    let mut device_free = vec![0u64; devices.len()];
    let mut assignments = Vec::with_capacity(n);

    for id in order {
        let task = &graph.tasks[id.0];
        sizes[id.0] = output_size(task, &sizes);
        let ready = task.inputs.iter().map(|t| finish[t.0]).max().unwrap_or(0);

        let mut best: Option<(usize, u64, u64)> = None;
        for (d, device) in devices.iter().enumerate() {
            if !device.supports(task.op) {
                continue;
            }
            let start = ready.max(device_free[d]);
            let end = start + device.cost(task.op, sizes[id.0]);
            if best.is_none_or(|(_, _, best_end)| end < best_end) {
                best = Some((d, start, end));
            }
        }
        let Some((device, start, end)) = best else {
            return Err(ScheduleError::NoDevice(id));
        };
        device_free[device] = end;
        finish[id.0] = end;
        assignments.push(Assignment {
            task: id,
            device,
            start,
            finish: end,
        });
    }

    let makespan = assignments.iter().map(|a| a.finish).max().unwrap_or(0);
    Ok(Schedule {
        assignments,
        makespan,
    })
}

/// Runs a schedule on the devices it was planned for, and returns each task's result.
///
/// Results are indexed by task id. Constant tasks take their data from the graph.
pub fn execute(
    graph: &Graph,
    plan: &Schedule,
    devices: &mut [Box<dyn Device>],
) -> Result<Vec<Vec<f32>>, ScheduleError> {
    let n = graph.tasks.len();
    let mut results: Vec<Vec<f32>> = vec![Vec::new(); n];
    for a in &plan.assignments {
        let task = &graph.tasks[a.task.0];
        let device = devices
            .get_mut(a.device)
            .ok_or(ScheduleError::NoDevice(a.task))?;
        let out = match &task.constant {
            Some(data) => {
                let inputs: Vec<&[f32]> = data.iter().map(Vec::as_slice).collect();
                device.run(task.op, &inputs)
            }
            None => {
                let inputs: Vec<&[f32]> = task
                    .inputs
                    .iter()
                    .map(|t| results[t.0].as_slice())
                    .collect();
                device.run(task.op, &inputs)
            }
        };
        results[a.task.0] = out.ok_or(ScheduleError::Refused(a.task))?;
    }
    Ok(results)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::device::{CpuDevice, MockAccelerator};

    fn cpu() -> Box<dyn Device> {
        Box::new(CpuDevice::default())
    }

    fn npu(ops: &[Op]) -> Box<dyn Device> {
        Box::new(MockAccelerator::new(ops))
    }

    fn pipeline() -> Graph {
        let mut g = Graph::new();
        let a: Vec<f32> = (0..2000).map(|i| i as f32 - 1000.0).collect();
        let b: Vec<f32> = (0..2000).map(|i| (i % 7) as f32).collect();
        let x = g.constant(Op::Add, vec![a, b]);
        let y = g.node(Op::Relu, &[x]);
        let z = g.node(Op::Mul, &[y, y]);
        let _s = g.node(Op::Sum, &[z]);
        g
    }

    #[test]
    fn mixed_schedule_matches_cpu_only_results() {
        let g = pipeline();
        let mut only_cpu: Vec<Box<dyn Device>> = vec![cpu()];
        let plan_cpu = schedule(&g, &only_cpu).unwrap();
        let expected = execute(&g, &plan_cpu, &mut only_cpu).unwrap();

        let mut mixed: Vec<Box<dyn Device>> = vec![cpu(), npu(&[Op::Add, Op::Mul])];
        let plan = schedule(&g, &mixed).unwrap();
        let got = execute(&g, &plan, &mut mixed).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn accelerator_takes_only_the_operations_it_supports() {
        let g = pipeline();
        let devices: Vec<Box<dyn Device>> = vec![cpu(), npu(&[Op::Add, Op::Mul])];
        let plan = schedule(&g, &devices).unwrap();
        for a in &plan.assignments {
            let op = g.tasks[a.task.0].op;
            if a.device == 1 {
                assert!(matches!(op, Op::Add | Op::Mul));
            }
        }
        // The large Add should go to the accelerator, which is faster on big inputs.
        let add = plan.assignments.iter().find(|a| a.task.0 == 0).unwrap();
        assert_eq!(add.device, 1);
    }

    #[test]
    fn assignments_respect_dependencies() {
        let g = pipeline();
        let devices: Vec<Box<dyn Device>> = vec![cpu(), npu(&[Op::Add, Op::Mul])];
        let plan = schedule(&g, &devices).unwrap();
        for a in &plan.assignments {
            for input in &g.tasks[a.task.0].inputs {
                let dep = plan.assignments.iter().find(|x| x.task == *input).unwrap();
                assert!(a.start >= dep.finish);
            }
        }
        assert!(plan.makespan >= plan.assignments.iter().map(|a| a.finish).max().unwrap());
    }

    #[test]
    fn unsupported_operation_is_reported() {
        let g = pipeline();
        let devices: Vec<Box<dyn Device>> = vec![npu(&[Op::Add])];
        assert!(matches!(
            schedule(&g, &devices),
            Err(ScheduleError::NoDevice(_))
        ));
    }

    #[test]
    fn wrong_arity_is_a_graph_error() {
        let mut g = Graph::new();
        let x = g.constant(Op::Add, vec![vec![1.0]]);
        let _ = g.node(Op::Relu, &[x, x]);
        assert!(matches!(
            schedule(&g, &[cpu()]),
            Err(ScheduleError::Graph(GraphError::Arity(_)))
        ));
    }
}

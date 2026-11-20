//! Task graphs: operations and the data they depend on.

use alloc::vec::Vec;
use core::fmt;

use crate::device::Op;

/// Identifies a task in a [`Graph`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(pub usize);

/// Why a graph is not valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphError {
    /// A task has the wrong number of inputs for its operation.
    Arity(TaskId),
    /// A task refers to a task that does not exist, or to itself.
    BadInput(TaskId),
    /// The dependencies form a cycle.
    Cycle,
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphError::Arity(t) => write!(f, "task {} has the wrong number of inputs", t.0),
            GraphError::BadInput(t) => write!(f, "task {} refers to a missing task", t.0),
            GraphError::Cycle => f.write_str("the task graph has a cycle"),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Task {
    pub(crate) op: Op,
    pub(crate) inputs: Vec<TaskId>,
    /// Constant data for tasks with no task inputs. Any other task takes its input from other tasks.
    pub(crate) constant: Option<Vec<Vec<f32>>>,
}

/// A directed acyclic graph of operations on vectors.
///
/// A task's inputs are either the results of other tasks, or constant vectors
/// given when the task is added. Tasks are added in any order, but the graph
/// must be acyclic when it is scheduled.
///
/// ```
/// use tpt_dev_dispatch::{Graph, Op};
///
/// let mut g = Graph::new();
/// let x = g.constant(Op::Add, vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
/// let y = g.node(Op::Relu, &[x]);
/// assert_eq!(g.len(), 2);
/// let _ = y;
/// ```
#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub(crate) tasks: Vec<Task>,
}

impl Graph {
    /// An empty graph.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dispatch::Graph;
    ///
    /// let graph = Graph::new();
    /// assert!(graph.is_empty());
    /// ```
    pub const fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    /// Number of tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dispatch::{Graph, Op};
    ///
    /// let mut graph = Graph::new();
    /// graph.constant(Op::Sum, vec![vec![1.0, 2.0]]);
    /// assert_eq!(graph.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Returns `true` when the graph has no tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dispatch::{Graph, Op};
    ///
    /// let mut graph = Graph::new();
    /// assert!(graph.is_empty());
    /// graph.constant(Op::Sum, vec![vec![1.0]]);
    /// assert!(!graph.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Adds a task that takes the results of `inputs`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dispatch::{Graph, Op};
    ///
    /// let mut graph = Graph::new();
    /// let x = graph.constant(Op::Add, vec![vec![1.0], vec![2.0]]);
    /// let _relu = graph.node(Op::Relu, &[x]);
    /// assert_eq!(graph.len(), 2);
    /// ```
    pub fn node(&mut self, op: Op, inputs: &[TaskId]) -> TaskId {
        self.tasks.push(Task {
            op,
            inputs: inputs.to_vec(),
            constant: None,
        });
        TaskId(self.tasks.len() - 1)
    }

    /// Adds a task whose inputs are constant vectors.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dispatch::{Graph, Op};
    ///
    /// let mut graph = Graph::new();
    /// let _data = graph.constant(Op::Sum, vec![vec![1.0, 2.0, 3.0]]);
    /// assert_eq!(graph.len(), 1);
    /// ```
    pub fn constant(&mut self, op: Op, data: Vec<Vec<f32>>) -> TaskId {
        self.tasks.push(Task {
            op,
            inputs: Vec::new(),
            constant: Some(data),
        });
        TaskId(self.tasks.len() - 1)
    }

    /// Checks arities and references, and returns a topological order.
    ///
    /// This API cannot build a cycle, because a task can only refer to tasks
    /// added before it. The cycle check is kept as a defensive guard.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dispatch::{Graph, Op};
    ///
    /// let mut graph = Graph::new();
    /// let x = graph.constant(Op::Add, vec![vec![1.0], vec![2.0]]);
    /// let y = graph.node(Op::Relu, &[x]);
    /// assert_eq!(graph.order().unwrap(), vec![x, y]);
    /// ```
    pub fn order(&self) -> Result<Vec<TaskId>, GraphError> {
        let n = self.tasks.len();
        let mut indegree = alloc::vec![0usize; n];
        for (i, t) in self.tasks.iter().enumerate() {
            let id = TaskId(i);
            let given = match &t.constant {
                Some(data) => data.len(),
                None => t.inputs.len(),
            };
            if given != t.op.arity() {
                return Err(GraphError::Arity(id));
            }
            for input in &t.inputs {
                if input.0 >= n || input.0 == i {
                    return Err(GraphError::BadInput(id));
                }
                indegree[i] += 1;
            }
        }
        // Kahn's algorithm, over the tasks that depend on each finished task.
        let mut ready: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(i) = ready.pop() {
            order.push(TaskId(i));
            for (j, t) in self.tasks.iter().enumerate() {
                // A task may take the same input twice, so count occurrences.
                let uses = t.inputs.iter().filter(|inp| inp.0 == i).count();
                if uses > 0 {
                    indegree[j] -= uses;
                    if indegree[j] == 0 {
                        ready.push(j);
                    }
                }
            }
        }
        if order.len() != n {
            return Err(GraphError::Cycle);
        }
        Ok(order)
    }
}

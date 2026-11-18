//! Machine description and analysis.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use tpt_dev_solver::{Arena, Formula, Rational, Solver, Status, Var};

/// Identifies a state of a [`Machine`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateId(u32);

impl StateId {
    /// The state's index in declaration order.
    pub const fn index(&self) -> usize {
        self.0 as usize
    }
}

/// A transition from one state to another, triggered by an event.
///
/// The guard is a solver formula over the machine's variables. `None` means the
/// transition is always enabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    /// Source state.
    pub from: StateId,
    /// Target state.
    pub to: StateId,
    /// Name of the event that triggers the transition.
    pub event: String,
    /// When the transition may fire, or `None` if it always may.
    pub guard: Option<Formula>,
}

/// Why a state can get stuck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeadlockKind {
    /// The state has no outgoing transitions.
    NoTransitions,
    /// Every outgoing guard can be false together. `witness` gives a valuation
    /// of the machine's variables where none of them is enabled.
    GuardsIncomplete {
        /// Variable names and values, in declaration order.
        witness: Vec<(String, Rational)>,
    },
}

/// A reachable, non-final state that can get stuck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deadlock {
    /// The stuck state.
    pub state: StateId,
    /// Why it is stuck.
    pub kind: DeadlockKind,
    /// Events on a shortest path from the initial state to this state.
    pub trace: Vec<String>,
}

/// The result of [`Machine::analyze`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Analysis {
    /// States reachable from the initial state, ignoring guards.
    pub reachable: Vec<StateId>,
    /// States that no path from the initial state reaches. These are truly unreachable.
    pub unreachable: Vec<StateId>,
    /// Reachable, non-final states that can get stuck.
    pub deadlocks: Vec<Deadlock>,
}

impl Analysis {
    /// Returns `true` when no reachable state can get stuck.
    pub fn is_deadlock_free(&self) -> bool {
        self.deadlocks.is_empty()
    }
}

/// Errors from building or analysing a machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A state or formula does not belong to this machine.
    UnknownState,
    /// The solver could not decide a guard question (arithmetic overflow).
    Unknown,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::UnknownState => f.write_str("state is not part of this machine"),
            Error::Unknown => {
                f.write_str("the solver could not decide a guard (arithmetic overflow)")
            }
        }
    }
}

/// A finite state machine with arithmetic guards.
///
/// ```
/// use tpt_dev_automata::{Linear, Machine, Rational};
///
/// let mut m = Machine::new();
/// let x = m.declare("x");
/// let idle = m.state("idle");
/// let busy = m.state("busy");
/// m.mark_final(idle);
///
/// // idle -> busy when x > 0. In busy, "stop" fires when x <= 0 and "tick"
/// // fires when x > 0, so together the guards cover every value of x.
/// let pos = m.arena_mut().gt(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
/// let nonpos = m.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
/// m.transition(idle, busy, "start", Some(pos)).unwrap();
/// m.transition(busy, idle, "stop", Some(nonpos)).unwrap();
/// m.transition(busy, busy, "tick", Some(pos)).unwrap();
///
/// let report = m.analyze(idle).unwrap();
/// assert!(report.is_deadlock_free());
/// ```
pub struct Machine {
    solver: Solver,
    vars: Vec<Var>,
    states: Vec<String>,
    finals: Vec<bool>,
    transitions: Vec<Transition>,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    /// Creates an empty machine.
    pub const fn new() -> Self {
        Self {
            solver: Solver::new(),
            vars: Vec::new(),
            states: Vec::new(),
            finals: Vec::new(),
            transitions: Vec::new(),
        }
    }

    /// Declares a real variable that guards may use.
    pub fn declare(&mut self, name: &str) -> Var {
        let v = self.solver.declare(name);
        self.vars.push(v);
        v
    }

    /// Arena for building guard formulas over the declared variables.
    pub fn arena_mut(&mut self) -> &mut Arena {
        self.solver.arena_mut()
    }

    /// Adds a state and returns its id.
    pub fn state(&mut self, name: &str) -> StateId {
        self.states.push(String::from(name));
        self.finals.push(false);
        StateId((self.states.len() - 1) as u32)
    }

    /// Marks a state as final. A final state may legitimately have no way out.
    pub fn mark_final(&mut self, s: StateId) {
        if let Some(f) = self.finals.get_mut(s.index()) {
            *f = true;
        }
    }

    /// Name of a state.
    pub fn state_name(&self, s: StateId) -> Option<&str> {
        self.states.get(s.index()).map(String::as_str)
    }

    /// Adds a transition. Fails if a state is unknown.
    pub fn transition(
        &mut self,
        from: StateId,
        to: StateId,
        event: &str,
        guard: Option<Formula>,
    ) -> Result<(), Error> {
        if from.index() >= self.states.len() || to.index() >= self.states.len() {
            return Err(Error::UnknownState);
        }
        self.transitions.push(Transition {
            from,
            to,
            event: String::from(event),
            guard,
        });
        Ok(())
    }

    /// The transitions added so far.
    pub fn transitions(&self) -> &[Transition] {
        &self.transitions
    }

    /// Analyses the machine from `initial`.
    ///
    /// Reachability ignores guards, so a state reported reachable might not be
    /// reachable with the real variable values. A state reported unreachable
    /// really is unreachable. For each reachable, non-final state, the solver
    /// checks whether its outgoing guards can all be false at once. If they
    /// can, the solver's valuation is the witness.
    pub fn analyze(&mut self, initial: StateId) -> Result<Analysis, Error> {
        if initial.index() >= self.states.len() {
            return Err(Error::UnknownState);
        }
        let n = self.states.len();
        // Breadth-first search, remembering how each state was first reached.
        let mut parent: Vec<Option<(usize, usize)>> = vec![None; n];
        let mut seen = vec![false; n];
        let mut queue = VecDeque::new();
        seen[initial.index()] = true;
        queue.push_back(initial.index());
        let mut order = Vec::new();
        while let Some(s) = queue.pop_front() {
            order.push(s);
            for (ti, t) in self.transitions.iter().enumerate() {
                if t.from.index() == s && !seen[t.to.index()] {
                    seen[t.to.index()] = true;
                    parent[t.to.index()] = Some((s, ti));
                    queue.push_back(t.to.index());
                }
            }
        }

        let mut deadlocks = Vec::new();
        for &s in &order {
            if self.finals[s] {
                continue;
            }
            let outgoing: Vec<usize> = (0..self.transitions.len())
                .filter(|&i| self.transitions[i].from.index() == s)
                .collect();
            let kind = if outgoing.is_empty() {
                Some(DeadlockKind::NoTransitions)
            } else {
                self.guards_incomplete(&outgoing)?
            };
            if let Some(kind) = kind {
                deadlocks.push(Deadlock {
                    state: StateId(s as u32),
                    kind,
                    trace: self.trace_to(s, &parent),
                });
            }
        }

        let mut reachable: Vec<StateId> = order.iter().map(|&s| StateId(s as u32)).collect();
        reachable.sort();
        let unreachable = (0..n)
            .filter(|&s| !seen[s])
            .map(|s| StateId(s as u32))
            .collect();
        Ok(Analysis {
            reachable,
            unreachable,
            deadlocks,
        })
    }

    /// Asks the solver whether the outgoing guards of a state can all be false.
    fn guards_incomplete(&mut self, outgoing: &[usize]) -> Result<Option<DeadlockKind>, Error> {
        let mut guards = Vec::with_capacity(outgoing.len());
        for &i in outgoing {
            match self.transitions[i].guard {
                // An always-enabled transition means the state cannot get stuck.
                None => return Ok(None),
                Some(g) => guards.push(g),
            }
        }
        let arena = self.solver.arena_mut();
        let any = arena.or(guards);
        let none_enabled = arena.not(any);
        self.solver.push();
        self.solver.assert(none_enabled);
        let result = self.solver.check();
        let witness = match result {
            Ok(Status::Sat) => Some(
                self.vars
                    .iter()
                    .map(|&v| {
                        let value = self.solver.value(v).unwrap_or(Rational::ZERO);
                        (String::from(self.solver.arena().var_name(v)), value)
                    })
                    .collect::<Vec<_>>(),
            ),
            Ok(Status::Unsat) => None,
            Err(_) => {
                self.solver.pop();
                return Err(Error::Unknown);
            }
        };
        self.solver.pop();
        Ok(witness.map(|witness| DeadlockKind::GuardsIncomplete { witness }))
    }

    /// Event names along the BFS path from the initial state to `s`.
    fn trace_to(&self, s: usize, parent: &[Option<(usize, usize)>]) -> Vec<String> {
        let mut events = Vec::new();
        let mut cur = s;
        while let Some((prev, ti)) = parent[cur] {
            events.push(self.transitions[ti].event.clone());
            cur = prev;
        }
        events.reverse();
        events
    }
}

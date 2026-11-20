//! Deadlock freedom checked in a `const` context.
//!
//! The run-time analysis in [`Machine`](crate::Machine) accepts any guard the
//! solver can express. This module accepts a smaller guard language, which
//! `const fn` can evaluate without allocation:
//!
//! - a guard is a conjunction of atoms `x <op> c`, where `x` is a variable
//!   index, `<op>` is one of `<`, `<=`, `>`, `>=`, `==`, and `c` is an `i64`
//!   constant (a "box" guard);
//! - an empty conjunction means the transition is always enabled.
//!
//! A machine written in this form can be checked at compile time:
//!
//! ```
//! use tpt_dev_automata::const_check::{assert_deadlock_free, Atom, Cmp, Edge, Spec};
//!
//! const POS: &[Atom] = &[Atom::new(0, Cmp::Gt, 0)];
//! const NONPOS: &[Atom] = &[Atom::new(0, Cmp::Le, 0)];
//! const EDGES: &[Edge<'_>] = &[
//!     Edge { from: 0, to: 1, guard: POS },
//!     Edge { from: 0, to: 0, guard: NONPOS },
//! ];
//! // State 0 waits until x > 0. State 1 is final.
//! const SPEC: Spec<'_> = Spec { states: 2, vars: 1, initial: 0, finals: &[1], edges: EDGES };
//!
//! // Fails the build if a reachable, non-final state can get stuck.
//! const _: () = assert_deadlock_free(&SPEC);
//! ```
//!
//! # Method
//!
//! Reachability ignores guards, as it does at run time, so it over-approximates
//! the reachable states. For each reachable, non-final state, the guards of its
//! outgoing transitions are boxes. A box is a product of intervals, so the
//! real line is split, per variable, into single points and open gaps between
//! them. Each atom has the same truth value on a whole cell. Checking one
//! representative per cell, on every combination of cells, therefore decides
//! whether the guards cover all valuations. The representatives are `2c - 1`,
//! `2c` and `2c + 1` for each threshold `c`, with all values doubled so that the
//! open gaps contain integers.
//!
//! # Limits
//!
//! - Only integer thresholds. Rational thresholds and guards such as `x + y > 3`
//!   are outside this subset. Use [`Machine::analyze`](crate::Machine::analyze)
//!   for those.
//! - At most [`MAX_STATES`] states, [`MAX_VARS`] variables, [`MAX_ATOMS_PER_VAR`]
//!   atoms per variable in one state's outgoing guards, and [`MAX_GRID`] cells in
//!   the grid for one state. Larger machines get [`Verdict::TooLarge`].
//! - The check trusts the [`Spec`]. A transition left out of the spec is never
//!   seen, so the spec must match the real machine.
//!
//! The check agrees with the run-time analysis on every machine in this
//! subset. `tests/const_check.rs` tests that agreement on random machines.

/// Largest number of states the const check accepts.
pub const MAX_STATES: usize = 64;
/// Largest number of variables the const check accepts.
pub const MAX_VARS: usize = 4;
/// Largest number of atoms on one variable in one state's outgoing guards.
pub const MAX_ATOMS_PER_VAR: usize = 16;
/// Largest number of grid cells the check enumerates for one state.
pub const MAX_GRID: usize = 1 << 16;

const MAX_SAMPLES: usize = 3 * MAX_ATOMS_PER_VAR;

/// A comparison between a variable and a constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    /// `x < c`
    Lt,
    /// `x <= c`
    Le,
    /// `x > c`
    Gt,
    /// `x >= c`
    Ge,
    /// `x == c`
    Eq,
}

/// One comparison `x[var] <op> value`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Atom {
    var: usize,
    cmp: Cmp,
    value: i64,
}

impl Atom {
    /// Creates the atom `x[var] <cmp> value`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_automata::const_check::{Atom, Cmp};
    /// let a = Atom::new(1, Cmp::Le, 5);
    /// assert_eq!((a.var(), a.value()), (1, 5));
    /// assert_eq!(a.cmp(), Cmp::Le);
    /// ```
    pub const fn new(var: usize, cmp: Cmp, value: i64) -> Self {
        Self { var, cmp, value }
    }

    /// Index of the variable this atom constrains.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_automata::const_check::{Atom, Cmp};
    /// assert_eq!(Atom::new(2, Cmp::Gt, 0).var(), 2);
    /// ```
    pub const fn var(&self) -> usize {
        self.var
    }

    /// The comparison.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_automata::const_check::{Atom, Cmp};
    /// assert_eq!(Atom::new(0, Cmp::Eq, 3).cmp(), Cmp::Eq);
    /// ```
    pub const fn cmp(&self) -> Cmp {
        self.cmp
    }

    /// The constant the variable is compared with.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_automata::const_check::{Atom, Cmp};
    /// assert_eq!(Atom::new(0, Cmp::Ge, -4).value(), -4);
    /// ```
    pub const fn value(&self) -> i64 {
        self.value
    }

    /// Whether the atom holds at `p`, where `p` is the point's value doubled.
    const fn holds_at_doubled(&self, p: i128) -> bool {
        let c = 2 * (self.value as i128);
        match self.cmp {
            Cmp::Lt => p < c,
            Cmp::Le => p <= c,
            Cmp::Gt => p > c,
            Cmp::Ge => p >= c,
            Cmp::Eq => p == c,
        }
    }
}

/// A transition in a [`Spec`]. The guard is a conjunction of atoms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge<'a> {
    /// Source state index.
    pub from: usize,
    /// Target state index.
    pub to: usize,
    /// Guard atoms. An empty slice means the transition is always enabled.
    pub guard: &'a [Atom],
}

/// A machine in the const-checkable subset. States and variables are indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec<'a> {
    /// Number of states.
    pub states: usize,
    /// Number of variables.
    pub vars: usize,
    /// Initial state index.
    pub initial: usize,
    /// Indices of final states, which may have no way out.
    pub finals: &'a [usize],
    /// Transitions.
    pub edges: &'a [Edge<'a>],
}

/// The result of [`check_deadlock_free`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// No reachable, non-final state can get stuck.
    DeadlockFree,
    /// The state has no outgoing transitions.
    NoTransitions {
        /// The stuck state.
        state: usize,
    },
    /// Some valuation disables every outgoing guard of the state.
    GuardsIncomplete {
        /// The stuck state.
        state: usize,
    },
    /// The spec refers to a state, variable or atom that does not exist.
    OutOfRange,
    /// The spec is larger than the const check's limits.
    TooLarge,
}

/// Checks a machine for deadlock freedom in a `const` context.
///
/// This is the same check as [`assert_deadlock_free`], but it returns the
/// verdict so that callers can inspect it.
///
/// # Examples
///
/// ```
/// use tpt_dev_automata::const_check::{check_deadlock_free, Atom, Cmp, Edge, Spec, Verdict};
///
/// const POS: &[Atom] = &[Atom::new(0, Cmp::Gt, 0)];
/// const EDGES: &[Edge<'_>] = &[Edge { from: 0, to: 1, guard: POS }];
/// const SPEC: Spec<'_> = Spec { states: 2, vars: 1, initial: 0, finals: &[1], edges: EDGES };
///
/// // The guard x > 0 can be false, so state 0 can get stuck.
/// assert_eq!(check_deadlock_free(&SPEC), Verdict::GuardsIncomplete { state: 0 });
/// ```
pub const fn check_deadlock_free(spec: &Spec<'_>) -> Verdict {
    if spec.states > MAX_STATES || spec.vars > MAX_VARS {
        return Verdict::TooLarge;
    }
    if spec.initial >= spec.states {
        return Verdict::OutOfRange;
    }
    let mut i = 0;
    while i < spec.finals.len() {
        if spec.finals[i] >= spec.states {
            return Verdict::OutOfRange;
        }
        i += 1;
    }
    i = 0;
    while i < spec.edges.len() {
        let e = spec.edges[i];
        if e.from >= spec.states || e.to >= spec.states {
            return Verdict::OutOfRange;
        }
        let mut j = 0;
        while j < e.guard.len() {
            if e.guard[j].var >= spec.vars {
                return Verdict::OutOfRange;
            }
            j += 1;
        }
        i += 1;
    }

    // Reachability ignores guards, as the run-time analysis does.
    let mut seen = [false; MAX_STATES];
    let mut queue = [0usize; MAX_STATES];
    let mut head = 0;
    let mut tail = 1;
    seen[spec.initial] = true;
    queue[0] = spec.initial;
    while head < tail {
        let s = queue[head];
        head += 1;
        let mut k = 0;
        while k < spec.edges.len() {
            let e = spec.edges[k];
            if e.from == s && !seen[e.to] {
                seen[e.to] = true;
                queue[tail] = e.to;
                tail += 1;
            }
            k += 1;
        }
    }

    let mut s = 0;
    while s < spec.states {
        if seen[s] && !is_final(spec, s) {
            let verdict = check_state(spec, s);
            if !matches!(verdict, Verdict::DeadlockFree) {
                return verdict;
            }
        }
        s += 1;
    }
    Verdict::DeadlockFree
}

/// Fails compilation, in a `const` context, if the machine can deadlock.
///
/// Use it as `const _: () = assert_deadlock_free(&SPEC);`. The message names the
/// kind of failure but not the state. Call [`check_deadlock_free`] at run time
/// to get the state.
///
/// # Examples
///
/// ```
/// use tpt_dev_automata::const_check::{assert_deadlock_free, Atom, Cmp, Edge, Spec};
///
/// const GO: &[Atom] = &[Atom::new(0, Cmp::Ge, 0)];
/// const STOP: &[Atom] = &[Atom::new(0, Cmp::Lt, 0)];
/// const EDGES: &[Edge<'_>] = &[
///     Edge { from: 0, to: 1, guard: GO },
///     Edge { from: 0, to: 0, guard: STOP },
/// ];
/// const SPEC: Spec<'_> = Spec { states: 2, vars: 1, initial: 0, finals: &[1], edges: EDGES };
///
/// // The two guards cover every value of x, so this compiles.
/// const _: () = assert_deadlock_free(&SPEC);
/// ```
#[allow(clippy::panic)]
pub const fn assert_deadlock_free(spec: &Spec<'_>) {
    match check_deadlock_free(spec) {
        Verdict::DeadlockFree => {}
        Verdict::NoTransitions { .. } => {
            panic!("deadlock: a reachable, non-final state has no outgoing transition")
        }
        Verdict::GuardsIncomplete { .. } => {
            panic!("deadlock: outgoing guards of a reachable, non-final state can all be false")
        }
        Verdict::OutOfRange => {
            panic!("machine spec refers to a state, variable or atom that does not exist")
        }
        Verdict::TooLarge => panic!(
            "machine is outside the const-checkable subset (states, variables, atoms or grid size)"
        ),
    }
}

const fn is_final(spec: &Spec<'_>, s: usize) -> bool {
    let mut i = 0;
    while i < spec.finals.len() {
        if spec.finals[i] == s {
            return true;
        }
        i += 1;
    }
    false
}

/// Whether the outgoing guards of state `s` cover every valuation.
const fn check_state(spec: &Spec<'_>, s: usize) -> Verdict {
    let mut outgoing = 0;
    let mut k = 0;
    while k < spec.edges.len() {
        let e = spec.edges[k];
        if e.from == s {
            if e.guard.is_empty() {
                return Verdict::DeadlockFree;
            }
            outgoing += 1;
        }
        k += 1;
    }
    if outgoing == 0 {
        return Verdict::NoTransitions { state: s };
    }

    // Per variable, the doubled sample points, and how many there are. A
    // variable no outgoing guard mentions gets one point, so it does not
    // multiply the grid.
    let mut samples = [[0i128; MAX_SAMPLES]; MAX_VARS];
    let mut count = [1usize; MAX_VARS];
    let mut total: usize = 1;
    let mut v = 0;
    while v < spec.vars {
        let mut n = 0;
        k = 0;
        while k < spec.edges.len() {
            let e = spec.edges[k];
            if e.from == s {
                let mut j = 0;
                while j < e.guard.len() {
                    let a = e.guard[j];
                    if a.var == v {
                        if n >= MAX_ATOMS_PER_VAR {
                            return Verdict::TooLarge;
                        }
                        let d = 2 * (a.value as i128);
                        samples[v][3 * n] = d - 1;
                        samples[v][3 * n + 1] = d;
                        samples[v][3 * n + 2] = d + 1;
                        n += 1;
                    }
                    j += 1;
                }
            }
            k += 1;
        }
        if n > 0 {
            count[v] = 3 * n;
            total *= count[v];
            if total > MAX_GRID {
                return Verdict::TooLarge;
            }
        }
        v += 1;
    }

    // Walk every grid point, decoding the index in mixed radix.
    let mut idx = 0;
    while idx < total {
        let mut rem = idx;
        let mut point = [0i128; MAX_VARS];
        v = 0;
        while v < spec.vars {
            point[v] = samples[v][rem % count[v]];
            rem /= count[v];
            v += 1;
        }
        if !covered(spec, s, &point) {
            return Verdict::GuardsIncomplete { state: s };
        }
        idx += 1;
    }
    Verdict::DeadlockFree
}

/// Whether some outgoing transition of `s` is enabled at `point` (doubled values).
const fn covered(spec: &Spec<'_>, s: usize, point: &[i128; MAX_VARS]) -> bool {
    let mut k = 0;
    while k < spec.edges.len() {
        let e = spec.edges[k];
        if e.from == s {
            let mut all = true;
            let mut j = 0;
            while j < e.guard.len() {
                let a = e.guard[j];
                if !a.holds_at_doubled(point[a.var]) {
                    all = false;
                    break;
                }
                j += 1;
            }
            if all {
                return true;
            }
        }
        k += 1;
    }
    false
}

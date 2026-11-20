//! Inductive-invariant search over transition systems with linear arithmetic.
//!
//! The search has three stages:
//!
//! 1. **Sample.** Random walks from the initial states record reachable states.
//!    A walk that reaches a state violating the property is a counterexample,
//!    and the search reports it as [`Outcome::Violated`].
//! 2. **Mine.** Linear bounds that hold on every sampled state become
//!    candidates. The property's own atoms are candidates too.
//! 3. **Filter (Houdini).** Candidates that fail initiation (`init => c`) are
//!    dropped. Then candidates that fail consecution (`inv and T => c'`) are
//!    dropped, repeating until nothing changes. The survivors form the largest
//!    inductive subset of the candidates, so if any subset proves the property,
//!    the survivors do too.
//!
//! The solver decides linear real arithmetic. Integer systems are checked over
//! the reals. That is sound: every integer run is also a real run, so an
//! invariant that is inductive over the reals holds on all integer runs. It can
//! miss invariants that only hold over the integers.

use alloc::format;
use alloc::vec::Vec;

use tpt_dev_solver::{Arena, Formula, Linear, Rational, Solver, Status, Unknown, Var};

use crate::invariant::Candidate;
use crate::rng::{Rng, Seed};

/// The relation in an [`Atom`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rel {
    Le,
    Ge,
    Eq,
}

/// A linear atom `sum(c_i * x_i) <= k`, `>= k` or `== k` over the state variables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Atom {
    terms: Vec<(usize, i64)>,
    bound: i64,
    rel: Rel,
}

impl Atom {
    /// The atom `sum(c_i * x_i) <= bound`. Each pair is `(variable index, coefficient)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::Atom;
    ///
    /// // x0 + 2 * x1 <= 10
    /// let atom = Atom::le(&[(0, 1), (1, 2)], 10);
    /// assert_eq!(atom, Atom::le(&[(0, 1), (1, 2)], 10));
    /// assert_ne!(atom, Atom::ge(&[(0, 1), (1, 2)], 10));
    /// ```
    pub fn le(terms: &[(usize, i64)], bound: i64) -> Self {
        Self::new(terms, bound, Rel::Le)
    }

    /// The atom `sum(c_i * x_i) >= bound`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::Atom;
    ///
    /// // x0 >= 3
    /// let atom = Atom::ge(&[(0, 1)], 3);
    /// assert_eq!(atom, Atom::ge(&[(0, 1)], 3));
    /// assert_ne!(atom, Atom::le(&[(0, 1)], 3));
    /// ```
    pub fn ge(terms: &[(usize, i64)], bound: i64) -> Self {
        Self::new(terms, bound, Rel::Ge)
    }

    /// The atom `sum(c_i * x_i) == bound`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::Atom;
    ///
    /// // x0 == 0
    /// let atom = Atom::eq(&[(0, 1)], 0);
    /// assert_eq!(atom, Atom::eq(&[(0, 1)], 0));
    /// assert_ne!(atom, Atom::le(&[(0, 1)], 0));
    /// ```
    pub fn eq(terms: &[(usize, i64)], bound: i64) -> Self {
        Self::new(terms, bound, Rel::Eq)
    }

    fn new(terms: &[(usize, i64)], bound: i64, rel: Rel) -> Self {
        Self {
            terms: terms.to_vec(),
            bound,
            rel,
        }
    }

    /// Whether the atom holds at `state`. `None` if the arithmetic overflows.
    fn holds(&self, state: &[Rational]) -> Option<bool> {
        let lhs = dot(&self.terms, state)?;
        let rhs = Rational::from_int(i128::from(self.bound));
        Some(match self.rel {
            Rel::Le => lhs <= rhs,
            Rel::Ge => lhs >= rhs,
            Rel::Eq => lhs == rhs,
        })
    }

    fn formula(&self, arena: &mut Arena, vars: &[Var]) -> Option<Formula> {
        let lhs = lin(&self.terms, 0, vars)?;
        let rhs = Linear::constant(Rational::from_int(i128::from(self.bound)));
        match self.rel {
            Rel::Le => arena.le(&lhs, &rhs),
            Rel::Ge => arena.ge(&lhs, &rhs),
            Rel::Eq => arena.eq(&lhs, &rhs),
        }
    }

    /// The atom as candidates of the form `sum <= bound`. An equality gives two.
    fn as_candidates(&self) -> Vec<Candidate> {
        let bound = Rational::from_int(i128::from(self.bound));
        let neg = || -> Option<Candidate> {
            let mut coeffs = Vec::with_capacity(self.terms.len());
            for &(i, c) in &self.terms {
                coeffs.push((i, c.checked_neg()?));
            }
            Some(Candidate {
                coeffs,
                bound: Rational::from_int(i128::from(self.bound)).checked_neg()?,
            })
        };
        let pos = Candidate {
            coeffs: self.terms.clone(),
            bound,
        };
        match self.rel {
            Rel::Le => alloc::vec![pos],
            Rel::Ge => neg().into_iter().collect(),
            Rel::Eq => neg().into_iter().chain(core::iter::once(pos)).collect(),
        }
    }
}

/// An affine expression `sum(c_i * x_i) + k`, used for the new value of a variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Affine {
    terms: Vec<(usize, i64)>,
    constant: i64,
}

impl Affine {
    /// The expression `sum(c_i * x_i) + constant`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::Affine;
    ///
    /// // x0 + x1 + 1
    /// let next = Affine::new(&[(0, 1), (1, 1)], 1);
    /// assert_eq!(next, Affine::new(&[(0, 1), (1, 1)], 1));
    /// ```
    pub fn new(terms: &[(usize, i64)], constant: i64) -> Self {
        Self {
            terms: terms.to_vec(),
            constant,
        }
    }

    /// The expression `x_i`, which keeps a variable unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::Affine;
    ///
    /// // Keeps x2 unchanged.
    /// assert_eq!(Affine::var(2), Affine::new(&[(2, 1)], 0));
    /// ```
    pub fn var(i: usize) -> Self {
        Self::new(&[(i, 1)], 0)
    }

    fn eval(&self, state: &[Rational]) -> Option<Rational> {
        dot(&self.terms, state)?.checked_add(Rational::from_int(i128::from(self.constant)))
    }

    fn linear(&self, vars: &[Var]) -> Option<Linear> {
        lin(&self.terms, self.constant, vars)
    }
}

/// One guarded step: when every guard atom holds, each variable `x_i` becomes
/// `updates[i]`. Variables without a real change should use [`Affine::var`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    guard: Vec<Atom>,
    updates: Vec<Affine>,
}

impl Transition {
    /// A transition with the given guard and one update per state variable.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::{Affine, Atom, Transition};
    ///
    /// // While x0 <= 9, increment x0.
    /// let step = Transition::new(
    ///     vec![Atom::le(&[(0, 1)], 9)],
    ///     vec![Affine::new(&[(0, 1)], 1)],
    /// );
    /// assert_eq!(step, Transition::new(vec![Atom::le(&[(0, 1)], 9)], vec![Affine::new(&[(0, 1)], 1)]));
    /// ```
    pub fn new(guard: Vec<Atom>, updates: Vec<Affine>) -> Self {
        Self { guard, updates }
    }
}

/// Why a [`TransitionSystem`] is malformed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecError {
    /// An atom or update refers to a variable index outside `0..nvars`.
    VariableOutOfRange,
    /// A transition does not give exactly one update per state variable.
    UpdateCount,
}

impl core::fmt::Display for SpecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::VariableOutOfRange => f.write_str("variable index out of range"),
            Self::UpdateCount => f.write_str("transition does not update every variable"),
        }
    }
}

/// A transition system: an initial predicate, guarded linear transitions, and a
/// property. Each predicate is a conjunction of atoms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionSystem {
    nvars: usize,
    init: Vec<Atom>,
    transitions: Vec<Transition>,
    property: Vec<Atom>,
}

impl TransitionSystem {
    /// Checks the variable indices and update counts, then builds the system.
    ///
    /// An empty `transitions` list means no step is possible.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::{Affine, Atom, SpecError, Transition, TransitionSystem};
    ///
    /// // One counter: start at 0, count up while x0 <= 9, and check x0 <= 10.
    /// let system = TransitionSystem::new(
    ///     1,
    ///     vec![Atom::eq(&[(0, 1)], 0)],
    ///     vec![Transition::new(vec![Atom::le(&[(0, 1)], 9)], vec![Affine::new(&[(0, 1)], 1)])],
    ///     vec![Atom::le(&[(0, 1)], 10)],
    /// )
    /// .unwrap();
    /// assert_eq!(system.nvars(), 1);
    ///
    /// // A transition that does not update every variable is refused.
    /// let bad = TransitionSystem::new(1, vec![], vec![Transition::new(vec![], vec![])], vec![]);
    /// assert_eq!(bad, Err(SpecError::UpdateCount));
    /// ```
    pub fn new(
        nvars: usize,
        init: Vec<Atom>,
        transitions: Vec<Transition>,
        property: Vec<Atom>,
    ) -> Result<Self, SpecError> {
        let in_range = |terms: &[(usize, i64)]| terms.iter().all(|&(i, _)| i < nvars);
        for a in init.iter().chain(property.iter()) {
            if !in_range(&a.terms) {
                return Err(SpecError::VariableOutOfRange);
            }
        }
        for t in &transitions {
            for a in &t.guard {
                if !in_range(&a.terms) {
                    return Err(SpecError::VariableOutOfRange);
                }
            }
            if t.updates.len() != nvars {
                return Err(SpecError::UpdateCount);
            }
            for u in &t.updates {
                if !in_range(&u.terms) {
                    return Err(SpecError::VariableOutOfRange);
                }
            }
        }
        Ok(Self {
            nvars,
            init,
            transitions,
            property,
        })
    }

    /// The number of state variables.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::TransitionSystem;
    ///
    /// let system = TransitionSystem::new(2, vec![], vec![], vec![]).unwrap();
    /// assert_eq!(system.nvars(), 2);
    /// ```
    pub fn nvars(&self) -> usize {
        self.nvars
    }
}

/// Settings for [`find_invariant`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchConfig {
    /// Seed for the random walks. The same seed gives the same samples.
    pub seed: Seed,
    /// Number of random walks, each started from a random initial state.
    pub walks: usize,
    /// Maximum number of steps per walk.
    pub depth: usize,
    /// Largest magnitude of the random lower bounds on initial values.
    pub spread: i64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            seed: Seed(1),
            walks: 16,
            depth: 32,
            spread: 10,
        }
    }
}

/// The result of [`find_invariant`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The property holds in every reachable state. `invariant` is a set of
    /// candidate bounds that the solver confirmed: it holds initially, it is
    /// preserved by every transition, and it implies the property.
    Proven {
        /// The inductive invariant, as a conjunction of bounds.
        invariant: Vec<Candidate>,
    },
    /// A reachable state violates the property. `trace` starts at an initial
    /// state, follows the transitions, and ends at the violating state.
    Violated {
        /// The states along the walk, one value per variable each.
        trace: Vec<Vec<Rational>>,
    },
    /// The initial predicate has no model, so no state is reachable and the
    /// property holds vacuously.
    Vacuous,
    /// No inductive invariant among the candidates implies the property. The
    /// property may still hold. This is not a refutation.
    NoInvariant {
        /// How many candidates were considered.
        candidates: usize,
        /// Whether the solver returned `Unknown` for some check.
        unknown: bool,
    },
}

/// Searches for an inductive invariant that implies the property.
///
/// Returns [`Outcome::Proven`] only when every check the solver made was
/// `Unsat`. A solver `Unknown` drops the candidate or ends the search without a
/// proof.
///
/// ```
/// use tpt_dev_test::{Affine, Atom, Outcome, SearchConfig, Transition, TransitionSystem, find_invariant};
///
/// // x starts at 0 and counts up while x <= 9. Prove x <= 10.
/// let system = TransitionSystem::new(
///     1,
///     vec![Atom::eq(&[(0, 1)], 0)],
///     vec![Transition::new(vec![Atom::le(&[(0, 1)], 9)], vec![Affine::new(&[(0, 1)], 1)])],
///     vec![Atom::le(&[(0, 1)], 10)],
/// ).unwrap();
/// assert!(matches!(find_invariant(&system, &SearchConfig::default()), Outcome::Proven { .. }));
/// ```
pub fn find_invariant(system: &TransitionSystem, config: &SearchConfig) -> Outcome {
    let n = system.nvars;
    let mut solver = Solver::new();
    let cur: Vec<Var> = (0..n).map(|i| solver.declare(&format!("x{i}"))).collect();
    let next: Vec<Var> = (0..n).map(|i| solver.declare(&format!("x{i}'"))).collect();

    let Some(init) = conjunction(solver.arena_mut(), &system.init, &cur) else {
        return unknown_result(0);
    };
    let Some(trans) = transition_relation(solver.arena_mut(), system, &cur, &next) else {
        return unknown_result(0);
    };

    // An unsatisfiable initial predicate leaves no reachable states.
    match sat(&mut solver, &[init]) {
        Ok(Status::Unsat) => return Outcome::Vacuous,
        Ok(Status::Sat) => {}
        Err(Unknown) => return unknown_result(0),
    }

    // Stage 1: sample reachable states. A violation is a real counterexample.
    let mut samples: Vec<Vec<Rational>> = Vec::new();
    let mut rng = Rng::new(config.seed);
    for _ in 0..config.walks {
        let Some(start) = random_init(&mut solver, &cur, init, &mut rng, config.spread) else {
            continue;
        };
        let mut trace = alloc::vec![start.clone()];
        match violates(&system.property, &start) {
            Some(true) => return Outcome::Violated { trace },
            Some(false) => {}
            None => continue,
        }
        samples.push(start.clone());
        let mut state = start;
        for _ in 0..config.depth {
            let Some(next_state) = step(system, &state, &mut rng) else {
                break;
            };
            state = next_state;
            match violates(&system.property, &state) {
                Some(true) => {
                    trace.push(state);
                    return Outcome::Violated { trace };
                }
                Some(false) => {}
                None => break,
            }
            samples.push(state.clone());
            trace.push(state.clone());
        }
    }

    // Stage 2: mine candidates from the samples, plus the property's own atoms.
    let mut candidates = mine(&samples, n);
    for a in &system.property {
        candidates.extend(a.as_candidates());
    }
    let total = candidates.len();

    let cur_f: Vec<Option<Formula>> = candidates
        .iter()
        .map(|c| c.to_formula(solver.arena_mut(), &cur))
        .collect();
    let next_f: Vec<Option<Formula>> = candidates
        .iter()
        .map(|c| c.to_formula(solver.arena_mut(), &next))
        .collect();
    let mut alive: Vec<bool> = cur_f.iter().map(Option::is_some).collect();
    let mut unknown = false;

    // Stage 3a: initiation. Drop candidates that some initial state violates.
    for (keep, f) in alive.iter_mut().zip(&cur_f) {
        let Some(f) = *f else {
            *keep = false;
            continue;
        };
        let neg = solver.arena_mut().not(f);
        match sat(&mut solver, &[init, neg]) {
            Ok(Status::Unsat) => {}
            Ok(Status::Sat) => *keep = false,
            Err(Unknown) => {
                *keep = false;
                unknown = true;
            }
        }
    }

    // Stage 3b: consecution, repeated until no candidate is dropped.
    loop {
        let mut base = alloc::vec![trans];
        base.extend(alive_formulas(&alive, &cur_f));
        let mut changed = false;
        for i in 0..total {
            if !alive.get(i).copied().unwrap_or(false) {
                continue;
            }
            let Some(Some(f)) = next_f.get(i).copied() else {
                continue;
            };
            let neg = solver.arena_mut().not(f);
            let mut query = base.clone();
            query.push(neg);
            match sat(&mut solver, &query) {
                Ok(Status::Unsat) => {}
                Ok(Status::Sat) => {
                    alive[i] = false;
                    changed = true;
                }
                Err(Unknown) => {
                    alive[i] = false;
                    changed = true;
                    unknown = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Stage 3c: the survivors must imply every property atom.
    let hyps = alive_formulas(&alive, &cur_f);
    for a in &system.property {
        let Some(f) = a.formula(solver.arena_mut(), &cur) else {
            return unknown_result(total);
        };
        let neg = solver.arena_mut().not(f);
        let mut query = hyps.clone();
        query.push(neg);
        match sat(&mut solver, &query) {
            Ok(Status::Unsat) => {}
            Ok(Status::Sat) => {
                return Outcome::NoInvariant {
                    candidates: total,
                    unknown,
                };
            }
            Err(Unknown) => return unknown_result(total),
        }
    }

    let invariant = candidates
        .into_iter()
        .zip(alive)
        .filter_map(|(c, keep)| keep.then_some(c))
        .collect();
    Outcome::Proven { invariant }
}

fn unknown_result(candidates: usize) -> Outcome {
    Outcome::NoInvariant {
        candidates,
        unknown: true,
    }
}

/// Checks `assertions` in a scope of their own. The solver is left unchanged.
fn sat(solver: &mut Solver, assertions: &[Formula]) -> Result<Status, Unknown> {
    solver.push();
    for &f in assertions {
        solver.assert(f);
    }
    let result = solver.check();
    solver.pop();
    result
}

fn alive_formulas(alive: &[bool], formulas: &[Option<Formula>]) -> Vec<Formula> {
    alive
        .iter()
        .zip(formulas)
        .filter_map(|(&keep, f)| if keep { *f } else { None })
        .collect()
}

fn conjunction(arena: &mut Arena, atoms: &[Atom], vars: &[Var]) -> Option<Formula> {
    let mut parts = Vec::with_capacity(atoms.len());
    for a in atoms {
        parts.push(a.formula(arena, vars)?);
    }
    Some(arena.and(parts))
}

/// The transition relation over `cur` (before) and `next` (after).
fn transition_relation(
    arena: &mut Arena,
    system: &TransitionSystem,
    cur: &[Var],
    next: &[Var],
) -> Option<Formula> {
    if system.transitions.is_empty() {
        return Some(arena.constant(false));
    }
    let mut options = Vec::with_capacity(system.transitions.len());
    for t in &system.transitions {
        let mut parts = Vec::with_capacity(t.guard.len() + t.updates.len());
        for a in &t.guard {
            parts.push(a.formula(arena, cur)?);
        }
        for (i, u) in t.updates.iter().enumerate() {
            let lhs = Linear::var(*next.get(i)?);
            let rhs = u.linear(cur)?;
            parts.push(arena.eq(&lhs, &rhs)?);
        }
        options.push(arena.and(parts));
    }
    Some(arena.or(options))
}

/// Picks random initial values, then asks the solver for a model. Random lower
/// bounds spread the samples out, and each one is kept only if the initial
/// predicate stays satisfiable with it.
fn random_init(
    solver: &mut Solver,
    cur: &[Var],
    init: Formula,
    rng: &mut Rng,
    spread: i64,
) -> Option<Vec<Rational>> {
    let mut kept = alloc::vec![init];
    for &v in cur {
        if !rng.bool() {
            continue;
        }
        let r = rng.range_i64(-spread.max(0), spread.max(0));
        let bound = Linear::constant(Rational::from_int(i128::from(r)));
        let f = solver.arena_mut().ge(&Linear::var(v), &bound)?;
        let mut trial = kept.clone();
        trial.push(f);
        if let Ok(Status::Sat) = sat(solver, &trial) {
            kept.push(f);
        }
    }
    solver.push();
    for &f in &kept {
        solver.assert(f);
    }
    let values = match solver.check() {
        Ok(Status::Sat) => Some(
            cur.iter()
                .map(|&v| solver.value(v).unwrap_or(Rational::ZERO))
                .collect(),
        ),
        _ => None,
    };
    solver.pop();
    values
}

/// Takes one enabled transition, chosen at random. `None` if no transition is
/// enabled, or if the arithmetic overflows.
fn step(system: &TransitionSystem, state: &[Rational], rng: &mut Rng) -> Option<Vec<Rational>> {
    let mut enabled = Vec::new();
    for t in &system.transitions {
        let mut all = true;
        for a in &t.guard {
            if !a.holds(state)? {
                all = false;
                break;
            }
        }
        if all {
            enabled.push(t);
        }
    }
    if enabled.is_empty() {
        return None;
    }
    let hi = i64::try_from(enabled.len() - 1).ok()?;
    let index = usize::try_from(rng.range_i64(0, hi)).ok()?;
    let t = enabled.get(index)?;
    t.updates.iter().map(|u| u.eval(state)).collect()
}

/// Whether some property atom fails at `state`. `None` if arithmetic overflows.
fn violates(property: &[Atom], state: &[Rational]) -> Option<bool> {
    for a in property {
        if !a.holds(state)? {
            return Some(true);
        }
    }
    Some(false)
}

/// Candidates `sum(c_i * x_i) <= max over samples`, for single variables and
/// pairs with coefficients in {-2, -1, 1, 2}. Each one holds on every sample.
fn mine(samples: &[Vec<Rational>], n: usize) -> Vec<Candidate> {
    let mut out = Vec::new();
    for coeffs in directions(n) {
        let probe = Candidate {
            coeffs: coeffs.clone(),
            bound: Rational::ZERO,
        };
        let mut best: Option<Rational> = None;
        let mut ok = true;
        for s in samples {
            match probe.at(s) {
                Some(v) => {
                    best = Some(match best {
                        Some(b) if b >= v => b,
                        _ => v,
                    });
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if let (true, Some(bound)) = (ok, best) {
            out.push(Candidate { coeffs, bound });
        }
    }
    out
}

fn directions(n: usize) -> Vec<Vec<(usize, i64)>> {
    const COEFFS: [i64; 4] = [1, -1, 2, -2];
    let mut out = Vec::new();
    for i in 0..n {
        for c in COEFFS {
            out.push(alloc::vec![(i, c)]);
        }
    }
    for i in 0..n {
        for j in (i + 1)..n {
            for a in COEFFS {
                for b in COEFFS {
                    out.push(alloc::vec![(i, a), (j, b)]);
                }
            }
        }
    }
    out
}

fn dot(terms: &[(usize, i64)], state: &[Rational]) -> Option<Rational> {
    let mut acc = Rational::ZERO;
    for &(i, c) in terms {
        let term = state
            .get(i)?
            .checked_mul(Rational::from_int(i128::from(c)))?;
        acc = acc.checked_add(term)?;
    }
    Some(acc)
}

fn lin(terms: &[(usize, i64)], constant: i64, vars: &[Var]) -> Option<Linear> {
    let mut acc = Linear::constant(Rational::from_int(i128::from(constant)));
    for &(i, c) in terms {
        let v = *vars.get(i)?;
        let term = Linear::var(v).scale(Rational::from_int(i128::from(c)))?;
        acc = acc.checked_add(&term)?;
    }
    Some(acc)
}

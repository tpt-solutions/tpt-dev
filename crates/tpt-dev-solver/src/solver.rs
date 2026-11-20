//! The solver: Boolean encoding, the DPLL(T) loop, incremental push/pop,
//! budgets, unsat cores and Farkas certificates.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use crate::Unknown;
use crate::proof::Proof;
use crate::rational::Rational;
use crate::sat::{Lit, Sat, SatResult};
use crate::simplex::{Check, DRat, Simplex};
use crate::term::{Arena, FixedArena, Formula, NodeView, Source, Var};

/// The answer to [`Solver::check`] when it is definite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The assertions have a model.
    Sat,
    /// The assertions have no model.
    Unsat,
}

/// Limits on the work a solver may do. When a limit is hit, the check returns
/// [`Unknown`] and the solver stays usable. Memory is bounded by the limits
/// on formula nodes and clauses, which are counted over the solver's life.
///
/// ```
/// use tpt_dev_solver::{Budget, Linear, Rational, Solver, Unknown};
///
/// let mut s = Solver::new();
/// let x = s.declare("x");
/// let f = s.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
/// s.assert(f);
/// // No formula nodes may be encoded, so the check gives up.
/// s.set_budget(Budget { max_formula_nodes: 0, ..Budget::UNLIMITED });
/// assert_eq!(s.check(), Err(Unknown));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Most formula nodes that may be encoded.
    pub max_formula_nodes: usize,
    /// Most clauses (input, Tseitin and learned) in the SAT database.
    pub max_clauses: usize,
    /// Most simplex pivots in one check.
    pub max_pivots: u64,
}

impl Budget {
    /// No limits.
    pub const UNLIMITED: Budget = Budget {
        max_formula_nodes: usize::MAX,
        max_clauses: usize::MAX,
        max_pivots: u64::MAX,
    };
}

impl Default for Budget {
    fn default() -> Self {
        Self::UNLIMITED
    }
}

/// A linear inequality as the theory sees it: `slack <= bound`, or `<` when strict.
pub(crate) struct TheoryAtom {
    /// Index of the row defining the slack variable (`n + row` in the tableau).
    pub(crate) row: usize,
    pub(crate) bound: Rational,
    pub(crate) strict: bool,
    pub(crate) var: u32,
}

/// Owns an [`Arena`] of formulas and a stack of asserted formulas.
///
/// The solver keeps its encoding and SAT state between calls. `check` encodes
/// only formulas it has not seen, and reuses the clauses it learned before.
/// The assertions are passed to the SAT core as assumptions, so `pop` removes
/// them without invalidating anything learned from them.
///
/// ```
/// use tpt_dev_solver::{Linear, Rational, Solver, Status};
///
/// let mut s = Solver::new();
/// let x = s.declare("x");
/// let one = Linear::constant(Rational::ONE);
/// let two = Linear::constant(Rational::from_int(2));
/// // x > 1 and x < 2
/// let f1 = s.arena_mut().gt(&Linear::var(x), &one).unwrap();
/// let f2 = s.arena_mut().lt(&Linear::var(x), &two).unwrap();
/// s.assert(f1);
/// s.assert(f2);
/// assert_eq!(s.check().unwrap(), Status::Sat);
/// let v = s.value(x).unwrap();
/// assert!(v > Rational::ONE && v < Rational::from_int(2));
/// ```
pub struct Solver {
    arena: Arena,
    assertions: Vec<Formula>,
    scopes: Vec<usize>,
    model: Option<Vec<Rational>>,
    core: Core,
    budget: Budget,
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

impl Solver {
    /// Creates a solver with no variables and no assertions.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Solver;
    ///
    /// let solver = Solver::new();
    /// assert!(solver.assertions().is_empty());
    /// ```
    pub const fn new() -> Self {
        Self {
            arena: Arena::new(),
            assertions: Vec::new(),
            scopes: Vec::new(),
            model: None,
            core: Core::new(),
            budget: Budget::UNLIMITED,
        }
    }

    /// Declares a real variable.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Solver;
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// assert_eq!(solver.arena().var_name(x), "x");
    /// ```
    pub fn declare(&mut self, name: &str) -> Var {
        self.model = None;
        self.arena.declare(name)
    }

    /// Read access to the formula arena.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Solver;
    ///
    /// let mut solver = Solver::new();
    /// solver.declare("x");
    /// assert_eq!(solver.arena().num_vars(), 1);
    /// ```
    pub fn arena(&self) -> &Arena {
        &self.arena
    }

    /// Write access to the formula arena, for building formulas.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let f = solver.arena_mut().constant(false);
    /// solver.assert(f);
    /// assert_eq!(solver.check(), Ok(Status::Unsat));
    /// ```
    pub fn arena_mut(&mut self) -> &mut Arena {
        &mut self.arena
    }

    /// Sets the limits for later checks. See [`Budget`].
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Budget, Solver};
    ///
    /// let mut solver = Solver::new();
    /// solver.set_budget(Budget { max_clauses: 10, ..Budget::UNLIMITED });
    /// assert_eq!(solver.budget().max_clauses, 10);
    /// ```
    pub fn set_budget(&mut self, budget: Budget) {
        self.budget = budget;
    }

    /// The current limits.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Budget, Solver};
    ///
    /// assert_eq!(Solver::new().budget(), Budget::UNLIMITED);
    /// ```
    pub fn budget(&self) -> Budget {
        self.budget
    }

    /// The assertions in the current scopes, in the order they were made.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Solver;
    ///
    /// let mut solver = Solver::new();
    /// let f = solver.arena_mut().constant(true);
    /// solver.assert(f);
    /// assert_eq!(solver.assertions().len(), 1);
    /// ```
    pub fn assertions(&self) -> &[Formula] {
        &self.assertions
    }

    /// Asserts `f` in the current scope.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Solver;
    ///
    /// let mut solver = Solver::new();
    /// let f = solver.arena_mut().constant(true);
    /// solver.assert(f);
    /// assert_eq!(solver.assertions(), &[f]);
    /// ```
    pub fn assert(&mut self, f: Formula) {
        self.model = None;
        self.assertions.push(f);
    }

    /// Opens a scope. Assertions made after this are removed by the matching `pop`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Solver;
    ///
    /// let mut solver = Solver::new();
    /// let base = solver.arena_mut().constant(true);
    /// solver.assert(base);
    /// solver.push();
    /// let extra = solver.arena_mut().constant(false);
    /// solver.assert(extra);
    /// assert_eq!(solver.assertions().len(), 2);
    /// assert!(solver.pop());
    /// assert_eq!(solver.assertions().len(), 1);
    /// ```
    pub fn push(&mut self) {
        self.scopes.push(self.assertions.len());
    }

    /// Closes the innermost scope. Returns `false` if there is no open scope.
    ///
    /// Clauses learned while the removed assertions were active stay in the
    /// solver. They are consequences of the formulas that the solver has
    /// already encoded, so they remain valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Solver;
    ///
    /// let mut solver = Solver::new();
    /// // No scope is open, so there is nothing to pop.
    /// assert!(!solver.pop());
    /// solver.push();
    /// assert!(solver.pop());
    /// ```
    pub fn pop(&mut self) -> bool {
        match self.scopes.pop() {
            Some(n) => {
                self.assertions.truncate(n);
                self.model = None;
                true
            }
            None => false,
        }
    }

    /// Decides whether the current assertions are satisfiable.
    ///
    /// Returns `Err(Unknown)` if an intermediate value does not fit in `i128`,
    /// or if a [`Budget`] limit is reached.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let f = solver.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
    /// solver.assert(f);
    /// assert_eq!(solver.check(), Ok(Status::Sat));
    /// ```
    pub fn check(&mut self) -> Result<Status, Unknown> {
        let roots = self.assertions.clone();
        self.decide(&roots)
    }

    /// Returns a minimal set of assertions that is unsatisfiable together, or
    /// `None` if the assertions are satisfiable.
    ///
    /// The result holds indices into the current assertion list, in increasing
    /// order. It is minimal: removing any one of them makes the rest
    /// satisfiable. This takes one solver call per assertion.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let t = solver.arena_mut().constant(true);
    /// let lo = solver.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
    /// let hi = solver.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
    /// for f in [t, lo, hi] {
    ///     solver.assert(f);
    /// }
    /// // The `true` assertion is not needed, so the core is the two bounds.
    /// assert_eq!(solver.unsat_core(), Ok(Some(vec![1, 2])));
    /// ```
    pub fn unsat_core(&mut self) -> Result<Option<Vec<usize>>, Unknown> {
        let all = self.assertions.clone();
        if self.decide(&all)? == Status::Sat {
            return Ok(None);
        }
        let mut keep: Vec<usize> = (0..all.len()).collect();
        let mut i = 0;
        while i < keep.len() {
            let trial: Vec<Formula> = keep
                .iter()
                .enumerate()
                .filter(|&(pos, _)| pos != i)
                .map(|(_, &idx)| all[idx])
                .collect();
            if self.decide(&trial)? == Status::Unsat {
                // Still unsat without this assertion, so it is not needed.
                keep.remove(i);
            } else {
                i += 1;
            }
        }
        self.model = None;
        Ok(Some(keep))
    }

    /// Returns a Farkas certificate that the current assertions are unsatisfiable.
    ///
    /// The certificate exists when the assertions flatten to linear literals
    /// (see [`Farkas`]). Then the simplex alone decides the problem, and the
    /// certificate is the combination of bounds that conflicts. It is checked
    /// before it is returned.
    ///
    /// Returns `Ok(None)` when the assertions are satisfiable, and also when
    /// they contain `or`, `not` over a compound formula, or `false`. Such
    /// problems need the full search, and this method does not prove them.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let lo = solver.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
    /// let hi = solver.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
    /// solver.assert(lo);
    /// solver.assert(hi);
    /// let cert = solver.unsat_certificate().unwrap().unwrap();
    /// assert!(solver.check_certificate(&cert));
    /// ```
    pub fn unsat_certificate(&mut self) -> Result<Option<Farkas>, Unknown> {
        let Some(lits) = literals(&self.arena, &self.assertions) else {
            return Ok(None);
        };
        let Some(cert) = farkas_conflict(&self.arena, &lits, self.budget.max_pivots)? else {
            return Ok(None);
        };
        if verify_farkas(&self.arena, &lits, &cert) {
            Ok(Some(cert))
        } else {
            // Unreachable by construction. Reporting Unknown keeps a wrong certificate from escaping.
            Err(Unknown)
        }
    }

    /// Returns a checkable proof that the current assertions are unsatisfiable,
    /// for any Boolean structure over linear atoms. The format is in
    /// [`crate::Proof`]. Check it with [`crate::check_proof`].
    ///
    /// Returns `Ok(None)` when the assertions are satisfiable. Returns
    /// `Err(Unknown)` when a budget is reached, or when the proof does not pass
    /// its own check. An `Unknown` never comes with a proof.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, check_proof};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let lo = solver.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
    /// let hi = solver.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
    /// solver.assert(lo);
    /// solver.assert(hi);
    /// let proof = solver.unsat_proof().unwrap().unwrap();
    /// assert!(check_proof(solver.arena(), solver.assertions(), &proof));
    /// ```
    pub fn unsat_proof(&mut self) -> Result<Option<Proof>, Unknown> {
        let Some(proof) = crate::proof::produce(&self.arena, &self.assertions, &self.budget)?
        else {
            return Ok(None);
        };
        if crate::proof::check_proof(&self.arena, &self.assertions, &proof) {
            Ok(Some(proof))
        } else {
            // Unreachable by construction. Reporting Unknown keeps a bad proof from escaping.
            Err(Unknown)
        }
    }

    /// Checks `cert` against the current assertions, with [`check_farkas`].
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let lo = solver.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
    /// let hi = solver.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
    /// solver.assert(lo);
    /// solver.assert(hi);
    /// let cert = solver.unsat_certificate().unwrap().unwrap();
    /// assert!(solver.check_certificate(&cert));
    /// ```
    pub fn check_certificate(&self, cert: &Farkas) -> bool {
        check_farkas(&self.arena, &self.assertions, cert)
    }

    /// Solves the given assertions. Stores the model if satisfiable.
    fn decide(&mut self, roots: &[Formula]) -> Result<Status, Unknown> {
        self.model = None;
        let (status, model) = self.core.check(&self.arena, roots, &self.budget)?;
        self.model = model;
        Ok(status)
    }

    /// The value of `v` in the model found by the last successful [`Solver::check`].
    ///
    /// Returns `None` if the last check was not `Sat`, or if assertions changed since then.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let f = solver.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
    /// solver.assert(f);
    /// assert_eq!(solver.check(), Ok(Status::Sat));
    /// assert!(solver.value(x).unwrap() >= Rational::ONE);
    /// ```
    pub fn value(&self, v: Var) -> Option<Rational> {
        self.model.as_ref().and_then(|m| m.get(v.index()).copied())
    }
}

impl<const VARS: usize, const NODES: usize, const TERMS: usize, const KIDS: usize>
    FixedArena<VARS, NODES, TERMS, KIDS>
{
    /// Decides the conjunction of `assertions`, under `budget`.
    ///
    /// The formulas are stored inline, so the arena allocates nothing. The
    /// search itself uses `alloc` for its working state.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Budget, FixedArena, Rational, Status};
    ///
    /// let mut arena: FixedArena<1, 4, 4, 4> = FixedArena::new();
    /// let x = arena.declare("x").unwrap();
    /// let lo = arena.atom(&[(x, Rational::from_int(-1))], Rational::from_int(-2), false).unwrap();
    /// let hi = arena.atom(&[(x, Rational::ONE)], Rational::ONE, false).unwrap();
    /// // x >= 2 and x <= 1 cannot both hold.
    /// assert_eq!(arena.check(&[lo, hi], &Budget::UNLIMITED), Ok(Status::Unsat));
    /// ```
    pub fn check(&self, assertions: &[Formula], budget: &Budget) -> Result<Status, Unknown> {
        Core::new().check(self, assertions, budget).map(|(s, _)| s)
    }

    /// Like [`FixedArena::check`]. On `Sat`, writes the value of each variable
    /// (by index) into `out`, up to its length.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Budget, FixedArena, Rational, Status};
    ///
    /// let mut arena: FixedArena<1, 4, 4, 4> = FixedArena::new();
    /// let x = arena.declare("x").unwrap();
    /// let lo = arena.atom(&[(x, Rational::from_int(-1))], Rational::from_int(-2), false).unwrap();
    /// let mut out = [Rational::ZERO; 1];
    /// assert_eq!(arena.check_model(&[lo], &Budget::UNLIMITED, &mut out), Ok(Status::Sat));
    /// assert!(out[0] >= Rational::from_int(2));
    /// ```
    pub fn check_model(
        &self,
        assertions: &[Formula],
        budget: &Budget,
        out: &mut [Rational],
    ) -> Result<Status, Unknown> {
        let (status, model) = Core::new().check(self, assertions, budget)?;
        if let Some(values) = model {
            for (o, v) in out.iter_mut().zip(values) {
                *o = v;
            }
        }
        Ok(status)
    }
}

/// The incremental DPLL(T) state: encoder, SAT core and the simplex tableau.
pub(crate) struct Core {
    enc: Encoder,
    sat: Sat,
    /// The tableau, kept between theory checks. Only its bounds change.
    simplex: Option<Simplex>,
    /// Variable and row counts the tableau was built for.
    simplex_key: (usize, usize),
    /// Whether the constant-true clause has been added.
    started: bool,
}

/// What one theory check found.
pub(crate) enum TheoryOutcome {
    /// A clause that blocks the assignment.
    Conflict(Vec<Lit>),
    /// A model for the real variables.
    Sat(Vec<Rational>),
}

impl Core {
    pub(crate) const fn new() -> Self {
        Self {
            enc: Encoder::new(),
            sat: Sat::new(),
            simplex: None,
            simplex_key: (0, 0),
            started: false,
        }
    }

    /// Decides the conjunction of `roots`. On `Sat`, also returns the values
    /// of the real variables.
    pub(crate) fn check<S: Source>(
        &mut self,
        src: &S,
        roots: &[Formula],
        budget: &Budget,
    ) -> Result<(Status, Option<Vec<Rational>>), Unknown> {
        if !self.started {
            self.started = true;
            // Variable 0 is the constant true.
            self.enc.clauses.push(vec![Lit::new(0, false)]);
        }
        let n = src.var_count();
        let mut assumptions = Vec::with_capacity(roots.len());
        for &f in roots {
            assumptions.push(self.enc.encode(src, f, budget.max_formula_nodes)?);
        }
        self.sat.ensure_vars(self.enc.next_var as usize);
        self.sat.set_max_clauses(budget.max_clauses);
        for c in self.enc.clauses.drain(..) {
            self.sat.add_clause(c);
        }

        // The tableau survives between checks unless variables or rows were added.
        let key = (n, self.enc.rows.len());
        if self.simplex.is_none() || self.simplex_key != key {
            self.simplex = Some(Simplex::new(n, &self.enc.rows, budget.max_pivots)?);
            self.simplex_key = key;
        }
        let simplex = self.simplex.as_mut().ok_or(Unknown)?;
        simplex.start_budget(budget.max_pivots);

        let atoms = &self.enc.atoms;
        let mut model: Option<Vec<Rational>> = None;
        let mut theory = |vals: &[i8]| -> Result<Option<Vec<Lit>>, Unknown> {
            match theory_check(simplex, n, atoms, vals)? {
                TheoryOutcome::Conflict(clause) => Ok(Some(clause)),
                TheoryOutcome::Sat(values) => {
                    model = Some(values);
                    Ok(None)
                }
            }
        };
        match self.sat.solve(&assumptions, &mut theory)? {
            SatResult::Sat => Ok((Status::Sat, model)),
            SatResult::Unsat => Ok((Status::Unsat, None)),
        }
    }
}

/// Checks the current full Boolean assignment against the linear theory.
/// Only the bounds are rebuilt. The tableau keeps the pivots of earlier checks.
pub(crate) fn theory_check(
    simplex: &mut Simplex,
    n: usize,
    atoms: &[TheoryAtom],
    vals: &[i8],
) -> Result<TheoryOutcome, Unknown> {
    simplex.reset_bounds();
    for atom in atoms {
        let v = vals[atom.var as usize];
        if v == 0 {
            continue;
        }
        let lit = Lit::new(atom.var, v == -1);
        let reason = lit.0;
        let slack = n + atom.row;
        let out = match (v == 1, atom.strict) {
            // slack <= bound, or slack < bound.
            (true, false) => {
                simplex.assert_upper(slack, DRat::new(atom.bound, Rational::ZERO), reason)?
            }
            (true, true) => simplex.assert_upper(
                slack,
                DRat::new(atom.bound, Rational::from_int(-1)),
                reason,
            )?,
            // not (slack <= bound) is slack > bound.
            (false, false) => {
                simplex.assert_lower(slack, DRat::new(atom.bound, Rational::from_int(1)), reason)?
            }
            // not (slack < bound) is slack >= bound.
            (false, true) => {
                simplex.assert_lower(slack, DRat::new(atom.bound, Rational::ZERO), reason)?
            }
        };
        if let Check::Conflict(reasons) = out {
            return Ok(TheoryOutcome::Conflict(negated_clause(&reasons)));
        }
    }
    match simplex.check()? {
        Check::Conflict(reasons) => Ok(TheoryOutcome::Conflict(negated_clause(&reasons))),
        Check::Sat => {
            let values = simplex.concrete_values().ok_or(Unknown)?;
            Ok(TheoryOutcome::Sat(values.into_iter().take(n).collect()))
        }
    }
}

/// The clause that forbids the conjunction of the given reason literals.
fn negated_clause(reasons: &[(u32, Rational)]) -> Vec<Lit> {
    reasons.iter().map(|&(r, _)| Lit(r).negate()).collect()
}

/// Converts formulas to CNF with the Tseitin transformation, and collects
/// the theory atoms and their defining rows. Kept across checks, so every
/// formula is encoded once.
pub(crate) struct Encoder {
    pub(crate) next_var: u32,
    /// Clauses not yet handed to the SAT core.
    pub(crate) clauses: Vec<Vec<Lit>>,
    /// Rows defining each distinct expression as a slack variable.
    pub(crate) rows: Vec<Vec<(usize, Rational)>>,
    slack_of: BTreeMap<Vec<(Var, Rational)>, usize>,
    pub(crate) atoms: Vec<TheoryAtom>,
    /// The Boolean variable of each encoded atom formula.
    pub(crate) atom_vars: Vec<(u32, Formula)>,
    /// Literal for each formula node already encoded.
    memo: BTreeMap<u32, Lit>,
}

impl Encoder {
    pub(crate) const fn new() -> Self {
        Self {
            // Variable 0 is the constant `true`.
            next_var: 1,
            clauses: Vec::new(),
            rows: Vec::new(),
            slack_of: BTreeMap::new(),
            atoms: Vec::new(),
            atom_vars: Vec::new(),
            memo: BTreeMap::new(),
        }
    }

    fn fresh(&mut self) -> Lit {
        let v = self.next_var;
        self.next_var += 1;
        Lit::new(v, false)
    }

    /// The literal for `f`. Fails if encoding would exceed `max_nodes`.
    pub(crate) fn encode<S: Source>(
        &mut self,
        src: &S,
        f: Formula,
        max_nodes: usize,
    ) -> Result<Lit, Unknown> {
        if let Some(&l) = self.memo.get(&f.0) {
            return Ok(l);
        }
        if self.memo.len() >= max_nodes {
            return Err(Unknown);
        }
        let lit = match src.view(f) {
            NodeView::Const(b) => {
                let t = Lit::new(0, false);
                if b { t } else { t.negate() }
            }
            NodeView::Atom {
                terms,
                bound,
                strict,
            } => {
                let lit = self.atom(terms.to_vec(), bound, strict);
                if lit.var() != 0 {
                    self.atom_vars.push((lit.var() as u32, f));
                }
                lit
            }
            NodeView::Not(g) => self.encode(src, g, max_nodes)?.negate(),
            NodeView::And(gs) => {
                let lits = self.encode_all(src, gs, max_nodes)?;
                self.and_gate(&lits)
            }
            NodeView::Or(gs) => {
                let lits = self.encode_all(src, gs, max_nodes)?;
                self.or_gate(&lits)
            }
        };
        self.memo.insert(f.0, lit);
        Ok(lit)
    }

    fn encode_all<S: Source>(
        &mut self,
        src: &S,
        gs: &[Formula],
        max_nodes: usize,
    ) -> Result<Vec<Lit>, Unknown> {
        let mut out = Vec::with_capacity(gs.len());
        for &g in gs {
            out.push(self.encode(src, g, max_nodes)?);
        }
        Ok(out)
    }

    /// A literal equivalent to `AND(lits)`.
    fn and_gate(&mut self, lits: &[Lit]) -> Lit {
        if lits.is_empty() {
            return Lit::new(0, false);
        }
        if lits.len() == 1 {
            return lits[0];
        }
        let x = self.fresh();
        // x -> each literal.
        for &l in lits {
            self.clauses.push(vec![x.negate(), l]);
        }
        // all literals -> x.
        let mut big: Vec<Lit> = lits.iter().map(|l| l.negate()).collect();
        big.push(x);
        self.clauses.push(big);
        x
    }

    /// A literal equivalent to `OR(lits)`.
    fn or_gate(&mut self, lits: &[Lit]) -> Lit {
        if lits.is_empty() {
            return Lit::new(0, true);
        }
        if lits.len() == 1 {
            return lits[0];
        }
        let x = self.fresh();
        // x -> some literal.
        let mut big: Vec<Lit> = lits.to_vec();
        big.push(x.negate());
        self.clauses.push(big);
        // each literal -> x.
        for &l in lits {
            self.clauses.push(vec![l.negate(), x]);
        }
        x
    }

    /// The literal for `sum(terms) <= bound` (or `<`).
    fn atom(&mut self, terms: Vec<(Var, Rational)>, bound: Rational, strict: bool) -> Lit {
        if terms.is_empty() {
            // 0 <= bound, or 0 < bound.
            let holds = if strict {
                !bound.is_negative() && !bound.is_zero()
            } else {
                !bound.is_negative()
            };
            let t = Lit::new(0, false);
            return if holds { t } else { t.negate() };
        }
        let row = match self.slack_of.get(&terms) {
            Some(&j) => j,
            None => {
                let j = self.rows.len();
                let row = terms.iter().map(|(v, c)| (v.index(), *c)).collect();
                self.rows.push(row);
                self.slack_of.insert(terms, j);
                j
            }
        };
        let var = self.next_var;
        self.next_var += 1;
        self.atoms.push(TheoryAtom {
            row,
            bound,
            strict,
            var,
        });
        Lit::new(var, false)
    }
}

/// A linear combination of literals that proves a set of linear constraints
/// unsatisfiable (a Farkas certificate).
///
/// # Format
///
/// The assertions are flattened in order. A top-level `and` is split
/// depth-first, `true` is dropped, and each remaining item must be an atom
/// `sum(c_i * x_i) <= k` or `sum(c_i * x_i) < k`, or the negation of one.
/// Literal `i` is the `i`-th item of this list.
///
/// A positive literal is the constraint `sum(c_i x_i) ⋈ k`. A negated
/// literal is `-sum(c_i x_i) ⋈' -k`, where `⋈'` is `<=` if the atom was `<`
/// and `<` if the atom was `<=`.
///
/// The certificate is a list of `(i, lambda_i)` pairs, with each `lambda_i > 0`
/// and each literal index used at most once. It is valid when both of these hold:
///
/// 1. For every variable, the sum of `lambda_i * c_i` over the list is zero.
/// 2. Write each bound as `a + b*delta`, where `a` is the bound and `b` is
///    `-1` for a strict constraint and `0` otherwise, with `delta` an
///    infinitesimal positive number. Then the sum of `lambda_i * (a_i + b_i*delta)`
///    is negative. Concretely, the real part is negative, or the real part is
///    zero and the `delta` part is negative.
///
/// If both hold, then `0 = sum lambda_i * lhs_i <= sum lambda_i * bound_i < 0`,
/// which no assignment can satisfy. [`check_farkas`] verifies exactly these
/// conditions, without using the solver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Farkas {
    coefficients: Vec<(usize, Rational)>,
}

impl Farkas {
    /// Builds a certificate from `(literal index, coefficient)` pairs. It
    /// is not checked until [`check_farkas`] is called.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Farkas, Rational};
    ///
    /// let cert = Farkas::new(vec![(0, Rational::ONE)]);
    /// assert_eq!(cert.coefficients(), &[(0, Rational::ONE)]);
    /// ```
    pub fn new(coefficients: Vec<(usize, Rational)>) -> Self {
        Self { coefficients }
    }

    /// The `(literal index, coefficient)` pairs.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Farkas, Rational};
    ///
    /// let cert = Farkas::new(vec![(0, Rational::ONE), (2, Rational::from_int(3))]);
    /// assert_eq!(cert.coefficients().len(), 2);
    /// assert_eq!(cert.coefficients()[1].0, 2);
    /// ```
    pub fn coefficients(&self) -> &[(usize, Rational)] {
        &self.coefficients
    }
}

/// Checks a Farkas certificate against `assertions`, without the solver.
///
/// Returns `true` only if `cert` follows the rules in [`Farkas`] for the
/// literals of `assertions`. It returns `false` on any arithmetic overflow.
///
/// ```
/// use tpt_dev_solver::{check_farkas, Farkas, Linear, Rational, Solver};
///
/// let mut s = Solver::new();
/// let x = s.declare("x");
/// let a = s.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
/// let b = s.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::from_int(2))).unwrap();
/// s.assert(a);
/// s.assert(b);
/// // x <= 1 and -x <= -2 sum to 0 <= -1.
/// let cert = s.unsat_certificate().unwrap().unwrap();
/// assert!(check_farkas(s.arena(), s.assertions(), &cert));
/// assert!(!check_farkas(s.arena(), s.assertions(), &Farkas::new(vec![(0, Rational::ONE)])));
/// ```
pub fn check_farkas(arena: &Arena, assertions: &[Formula], cert: &Farkas) -> bool {
    match literals(arena, assertions) {
        Some(lits) => verify_farkas(arena, &lits, cert),
        None => false,
    }
}

/// Flattens the assertions to `(atom, positive)` literals, as [`Farkas`] describes.
/// `None` if some assertion is not a conjunction of atoms and negated atoms.
fn literals<S: Source>(src: &S, assertions: &[Formula]) -> Option<Vec<(Formula, bool)>> {
    let mut out = Vec::new();
    for &f in assertions {
        collect_literals(src, f, &mut out)?;
    }
    Some(out)
}

fn collect_literals<S: Source>(src: &S, f: Formula, out: &mut Vec<(Formula, bool)>) -> Option<()> {
    match src.view(f) {
        NodeView::Const(true) => Some(()),
        NodeView::Atom { .. } => {
            out.push((f, true));
            Some(())
        }
        NodeView::Not(g) => match src.view(g) {
            NodeView::Atom { .. } => {
                out.push((g, false));
                Some(())
            }
            _ => None,
        },
        NodeView::And(gs) => {
            for &g in gs {
                collect_literals(src, g, out)?;
            }
            Some(())
        }
        NodeView::Const(false) | NodeView::Or(_) => None,
    }
}

/// Finds the simplex conflict for a conjunction of literals, as a certificate.
/// `Ok(None)` if the simplex finds the literals satisfiable.
pub(crate) fn farkas_conflict<S: Source>(
    src: &S,
    lits: &[(Formula, bool)],
    pivot_limit: u64,
) -> Result<Option<Farkas>, Unknown> {
    let n = src.var_count();
    let mut rows: Vec<Vec<(usize, Rational)>> = Vec::new();
    let mut slack_of: BTreeMap<Vec<(Var, Rational)>, usize> = BTreeMap::new();
    // (row, is_lower, bound, literal index) for each literal.
    let mut bounds: Vec<(usize, bool, DRat, u32)> = Vec::with_capacity(lits.len());
    for (i, &(f, pos)) in lits.iter().enumerate() {
        let NodeView::Atom {
            terms,
            bound,
            strict,
        } = src.view(f)
        else {
            return Ok(None);
        };
        let row = match slack_of.get(terms) {
            Some(&j) => j,
            None => {
                let j = rows.len();
                rows.push(terms.iter().map(|(v, c)| (v.index(), *c)).collect());
                slack_of.insert(terms.to_vec(), j);
                j
            }
        };
        let one = Rational::ONE;
        let zero = Rational::ZERO;
        let minus_one = Rational::from_int(-1);
        bounds.push(if pos {
            // F <= bound, or F < bound.
            (
                row,
                false,
                DRat::new(bound, if strict { minus_one } else { zero }),
                i as u32,
            )
        } else {
            // F > bound (from a non-strict atom), or F >= bound (from a strict one).
            (
                row,
                true,
                DRat::new(bound, if strict { zero } else { one }),
                i as u32,
            )
        });
    }
    let mut sx = Simplex::new(n, &rows, pivot_limit)?;
    for &(row, lower, b, i) in &bounds {
        let out = if lower {
            sx.assert_lower(n + row, b, i)?
        } else {
            sx.assert_upper(n + row, b, i)?
        };
        if let Check::Conflict(list) = out {
            return Ok(Some(farkas_from(&list)));
        }
    }
    match sx.check()? {
        Check::Conflict(list) => Ok(Some(farkas_from(&list))),
        Check::Sat => Ok(None),
    }
}

fn farkas_from(list: &[(u32, Rational)]) -> Farkas {
    Farkas {
        coefficients: list.iter().map(|&(r, c)| (r as usize, c)).collect(),
    }
}

/// The rules in [`Farkas`], checked directly on the literals.
pub(crate) fn verify_farkas<S: Source>(src: &S, lits: &[(Formula, bool)], cert: &Farkas) -> bool {
    if cert.coefficients.is_empty() {
        return false;
    }
    let mut sums: BTreeMap<Var, Rational> = BTreeMap::new();
    let mut real = Rational::ZERO;
    let mut delta = Rational::ZERO;
    let mut used = alloc::vec![false; lits.len()];
    for &(idx, lam) in &cert.coefficients {
        if !(lam > Rational::ZERO) {
            return false;
        }
        let Some(&(f, pos)) = lits.get(idx) else {
            return false;
        };
        if used[idx] {
            return false;
        }
        used[idx] = true;
        let NodeView::Atom {
            terms,
            bound,
            strict,
        } = src.view(f)
        else {
            return false;
        };
        let sign = if pos {
            Rational::ONE
        } else {
            Rational::from_int(-1)
        };
        for &(v, c) in terms {
            let Some(w) = lam.checked_mul(sign).and_then(|x| x.checked_mul(c)) else {
                return false;
            };
            let entry = sums.entry(v).or_insert(Rational::ZERO);
            let Some(s) = entry.checked_add(w) else {
                return false;
            };
            *entry = s;
        }
        let Some(b) = (if pos {
            Some(bound)
        } else {
            bound.checked_neg()
        }) else {
            return false;
        };
        let strict_here = if pos { strict } else { !strict };
        let d = if strict_here {
            Rational::from_int(-1)
        } else {
            Rational::ZERO
        };
        let (Some(r2), Some(d2)) = (
            lam.checked_mul(b).and_then(|x| real.checked_add(x)),
            lam.checked_mul(d).and_then(|x| delta.checked_add(x)),
        ) else {
            return false;
        };
        real = r2;
        delta = d2;
    }
    if sums.values().any(|v| !v.is_zero()) {
        return false;
    }
    real < Rational::ZERO || (real.is_zero() && delta < Rational::ZERO)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::term::Linear;

    fn int(n: i128) -> Linear {
        Linear::constant(Rational::from_int(n))
    }

    #[test]
    fn simple_bounds_sat_and_model() {
        let mut s = Solver::new();
        let x = s.declare("x");
        let f = s.arena_mut().le(&Linear::var(x), &int(3)).unwrap();
        let g = s.arena_mut().ge(&Linear::var(x), &int(1)).unwrap();
        s.assert(f);
        s.assert(g);
        assert_eq!(s.check().unwrap(), Status::Sat);
        let v = s.value(x).unwrap();
        assert!(v >= Rational::ONE && v <= Rational::from_int(3));
    }

    #[test]
    fn contradiction_is_unsat() {
        let mut s = Solver::new();
        let x = s.declare("x");
        let f = s.arena_mut().lt(&Linear::var(x), &int(0)).unwrap();
        let g = s.arena_mut().gt(&Linear::var(x), &int(0)).unwrap();
        s.assert(f);
        s.assert(g);
        assert_eq!(s.check().unwrap(), Status::Unsat);
        assert!(s.value(x).is_none());
    }

    #[test]
    fn disjunction_needs_boolean_search() {
        // (x <= 0 or x >= 10) and x > 5  => x >= 10
        let mut s = Solver::new();
        let x = s.declare("x");
        let a = s.arena_mut().le(&Linear::var(x), &int(0)).unwrap();
        let b = s.arena_mut().ge(&Linear::var(x), &int(10)).unwrap();
        let or = s.arena_mut().or(alloc::vec![a, b]);
        let c = s.arena_mut().gt(&Linear::var(x), &int(5)).unwrap();
        s.assert(or);
        s.assert(c);
        assert_eq!(s.check().unwrap(), Status::Sat);
        assert!(s.value(x).unwrap() >= Rational::from_int(10));
    }

    #[test]
    fn linear_system_with_two_variables() {
        // x + y = 10, x - y = 4  => x = 7, y = 3
        let mut s = Solver::new();
        let x = s.declare("x");
        let y = s.declare("y");
        let sum = Linear::var(x).checked_add(&Linear::var(y)).unwrap();
        let diff = Linear::var(x).checked_sub(&Linear::var(y)).unwrap();
        let f = s.arena_mut().eq(&sum, &int(10)).unwrap();
        let g = s.arena_mut().eq(&diff, &int(4)).unwrap();
        s.assert(f);
        s.assert(g);
        assert_eq!(s.check().unwrap(), Status::Sat);
        assert_eq!(s.value(x), Some(Rational::from_int(7)));
        assert_eq!(s.value(y), Some(Rational::from_int(3)));
    }

    #[test]
    fn push_and_pop_scope_assertions() {
        let mut s = Solver::new();
        let x = s.declare("x");
        let f = s.arena_mut().ge(&Linear::var(x), &int(5)).unwrap();
        s.assert(f);
        s.push();
        let g = s.arena_mut().le(&Linear::var(x), &int(4)).unwrap();
        s.assert(g);
        assert_eq!(s.check().unwrap(), Status::Unsat);
        assert!(s.pop());
        assert_eq!(s.check().unwrap(), Status::Sat);
        assert!(!s.pop());
    }

    #[test]
    fn implication_and_negation() {
        // (x > 0 => y > 0) and x > 0 and y < 0  is unsat.
        let mut s = Solver::new();
        let x = s.declare("x");
        let y = s.declare("y");
        let px = s.arena_mut().gt(&Linear::var(x), &int(0)).unwrap();
        let py = s.arena_mut().gt(&Linear::var(y), &int(0)).unwrap();
        let imp = s.arena_mut().implies(px, py);
        let ny = s.arena_mut().lt(&Linear::var(y), &int(0)).unwrap();
        s.assert(imp);
        s.assert(px);
        s.assert(ny);
        assert_eq!(s.check().unwrap(), Status::Unsat);
    }

    #[test]
    fn unsat_core_is_minimal() {
        // x > 5 and x < 3 conflict. y > 0 and y < 10 are harmless.
        let mut s = Solver::new();
        let x = s.declare("x");
        let y = s.declare("y");
        let a = s.arena_mut().gt(&Linear::var(x), &int(5)).unwrap();
        let b = s.arena_mut().gt(&Linear::var(y), &int(0)).unwrap();
        let c = s.arena_mut().lt(&Linear::var(x), &int(3)).unwrap();
        let d = s.arena_mut().lt(&Linear::var(y), &int(10)).unwrap();
        for f in [a, b, c, d] {
            s.assert(f);
        }
        assert_eq!(s.unsat_core().unwrap(), Some(vec![0, 2]));
        // The assertions themselves are unchanged.
        assert_eq!(s.check().unwrap(), Status::Unsat);
    }

    #[test]
    fn unsat_core_none_when_sat() {
        let mut s = Solver::new();
        let x = s.declare("x");
        let a = s.arena_mut().gt(&Linear::var(x), &int(5)).unwrap();
        s.assert(a);
        assert_eq!(s.unsat_core().unwrap(), None);
    }

    #[test]
    fn constant_formulas() {
        let mut s = Solver::new();
        let t = s.arena_mut().constant(true);
        s.assert(t);
        assert_eq!(s.check().unwrap(), Status::Sat);
        let f = s.arena_mut().constant(false);
        s.assert(f);
        assert_eq!(s.check().unwrap(), Status::Unsat);
    }

    #[test]
    fn clause_budget_gives_unknown() {
        let mut s = Solver::new();
        let x = s.declare("x");
        let a = s.arena_mut().le(&Linear::var(x), &int(0)).unwrap();
        let b = s.arena_mut().ge(&Linear::var(x), &int(10)).unwrap();
        let or = s.arena_mut().or(alloc::vec![a, b]);
        let c = s.arena_mut().gt(&Linear::var(x), &int(5)).unwrap();
        s.assert(or);
        s.assert(c);
        s.set_budget(Budget {
            max_clauses: 1,
            ..Budget::UNLIMITED
        });
        assert_eq!(s.check(), Err(Unknown));
        // Lifting the limit gives the answer.
        s.set_budget(Budget::UNLIMITED);
        assert_eq!(s.check().unwrap(), Status::Sat);
    }

    #[test]
    fn pivot_budget_gives_unknown() {
        // x + y <= 1, x >= 1, y >= 1 is unsat, but needs a pivot to see it.
        let mut s = Solver::new();
        let x = s.declare("x");
        let y = s.declare("y");
        let sum = Linear::var(x).checked_add(&Linear::var(y)).unwrap();
        let a = s.arena_mut().le(&sum, &int(1)).unwrap();
        let b = s.arena_mut().ge(&Linear::var(x), &int(1)).unwrap();
        let c = s.arena_mut().ge(&Linear::var(y), &int(1)).unwrap();
        for f in [a, b, c] {
            s.assert(f);
        }
        s.set_budget(Budget {
            max_pivots: 0,
            ..Budget::UNLIMITED
        });
        assert_eq!(s.check(), Err(Unknown));
        s.set_budget(Budget::UNLIMITED);
        assert_eq!(s.check().unwrap(), Status::Unsat);
    }

    #[test]
    fn farkas_certificate_for_simplex_conflict() {
        // x <= 1, x >= 2: the certificate is the two bounds with weight 1.
        let mut s = Solver::new();
        let x = s.declare("x");
        let a = s.arena_mut().le(&Linear::var(x), &int(1)).unwrap();
        let b = s.arena_mut().ge(&Linear::var(x), &int(2)).unwrap();
        s.assert(a);
        s.assert(b);
        let cert = s.unsat_certificate().unwrap().unwrap();
        assert!(s.check_certificate(&cert));
    }

    #[test]
    fn farkas_none_for_sat_or_boolean_structure() {
        let mut s = Solver::new();
        let x = s.declare("x");
        let a = s.arena_mut().le(&Linear::var(x), &int(1)).unwrap();
        s.assert(a);
        assert_eq!(s.unsat_certificate().unwrap(), None);
        let b = s.arena_mut().ge(&Linear::var(x), &int(2)).unwrap();
        let or = s.arena_mut().or(alloc::vec![a, b]);
        s.assert(or);
        assert_eq!(s.unsat_certificate().unwrap(), None);
    }
}

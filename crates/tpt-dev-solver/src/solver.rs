//! The solver: Boolean encoding, the DPLL(T) loop, push/pop and models.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use crate::Unknown;
use crate::rational::Rational;
use crate::sat::{Lit, Sat, SatResult};
use crate::simplex::{Check, DRat, Simplex};
use crate::term::{Arena, Formula, Node, Var};

/// The answer to [`Solver::check`] when it is definite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The assertions have a model.
    Sat,
    /// The assertions have no model.
    Unsat,
}

/// A linear inequality as the theory sees it: `slack <= bound`, or `<` when strict.
struct TheoryAtom {
    slack: usize,
    bound: Rational,
    strict: bool,
    var: u32,
}

/// Owns an [`Arena`] of formulas and a stack of asserted formulas.
///
/// `check` encodes the assertions to CNF and runs a DPLL(T) search. Each
/// call starts a fresh search, so `push` and `pop` only change the set of
/// assertions. That keeps them simple to get right.
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
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

impl Solver {
    /// Creates a solver with no variables and no assertions.
    pub const fn new() -> Self {
        Self {
            arena: Arena::new(),
            assertions: Vec::new(),
            scopes: Vec::new(),
            model: None,
        }
    }

    /// Declares a real variable.
    pub fn declare(&mut self, name: &str) -> Var {
        self.model = None;
        self.arena.declare(name)
    }

    /// Read access to the formula arena.
    pub fn arena(&self) -> &Arena {
        &self.arena
    }

    /// Write access to the formula arena, for building formulas.
    pub fn arena_mut(&mut self) -> &mut Arena {
        &mut self.arena
    }

    /// Asserts `f` in the current scope.
    pub fn assert(&mut self, f: Formula) {
        self.model = None;
        self.assertions.push(f);
    }

    /// Opens a scope. Assertions made after this are removed by the matching `pop`.
    pub fn push(&mut self) {
        self.scopes.push(self.assertions.len());
    }

    /// Closes the innermost scope. Returns `false` if there is no open scope.
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
    /// Returns `Err(Unknown)` if an intermediate value does not fit in `i128`.
    pub fn check(&mut self) -> Result<Status, Unknown> {
        let assertions = self.assertions.clone();
        self.decide(&assertions)
    }

    /// Returns a minimal set of assertions that is unsatisfiable together, or
    /// `None` if the assertions are satisfiable.
    ///
    /// The result holds indices into the current assertion list, in increasing
    /// order. It is minimal: removing any one of them makes the rest
    /// satisfiable. This takes one solver call per assertion.
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

    /// Solves the given assertions. Stores the model if satisfiable.
    fn decide(&mut self, assertions: &[Formula]) -> Result<Status, Unknown> {
        self.model = None;
        let n = self.arena.num_vars();
        let mut enc = Encoder::new(n);
        let mut roots = Vec::with_capacity(assertions.len());
        for &f in assertions {
            roots.push(enc.encode(&self.arena, f)?);
        }
        for r in roots {
            enc.clauses.push(vec![r]);
        }
        enc.clauses.push(vec![Lit::new(0, false)]);
        let mut sat = Sat::new(enc.next_var as usize);
        for c in enc.clauses {
            sat.add_clause(c);
        }

        let mut model: Option<Vec<Rational>> = None;
        let rows = &enc.rows;
        let atoms = &enc.atoms;
        let mut theory = |vals: &[i8]| -> Result<Option<Vec<Lit>>, Unknown> {
            let mut simplex = Simplex::new(n, rows)?;
            for atom in atoms {
                let v = vals[atom.var as usize];
                if v == 0 {
                    continue;
                }
                let lit = Lit::new(atom.var, v == -1);
                let reason = lit.0;
                let out = match (v == 1, atom.strict) {
                    // slack <= bound, or slack < bound.
                    (true, false) => simplex.assert_upper(
                        atom.slack,
                        DRat::new(atom.bound, Rational::ZERO),
                        reason,
                    )?,
                    (true, true) => simplex.assert_upper(
                        atom.slack,
                        DRat::new(atom.bound, Rational::from_int(-1)),
                        reason,
                    )?,
                    // not (slack <= bound) is slack > bound.
                    (false, false) => simplex.assert_lower(
                        atom.slack,
                        DRat::new(atom.bound, Rational::from_int(1)),
                        reason,
                    )?,
                    // not (slack < bound) is slack >= bound.
                    (false, true) => simplex.assert_lower(
                        atom.slack,
                        DRat::new(atom.bound, Rational::ZERO),
                        reason,
                    )?,
                };
                if let Check::Conflict(reasons) = out {
                    return Ok(Some(reasons.into_iter().map(|r| Lit(r).negate()).collect()));
                }
            }
            match simplex.check()? {
                Check::Conflict(reasons) => {
                    Ok(Some(reasons.into_iter().map(|r| Lit(r).negate()).collect()))
                }
                Check::Sat => {
                    let values = simplex.concrete_values().ok_or(Unknown)?;
                    model = Some(values.into_iter().take(n).collect());
                    Ok(None)
                }
            }
        };

        match sat.solve(&mut theory)? {
            SatResult::Sat => {
                self.model = model;
                Ok(Status::Sat)
            }
            SatResult::Unsat => Ok(Status::Unsat),
        }
    }

    /// The value of `v` in the model found by the last successful [`Solver::check`].
    ///
    /// Returns `None` if the last check was not `Sat`, or if assertions changed since then.
    pub fn value(&self, v: Var) -> Option<Rational> {
        self.model.as_ref().and_then(|m| m.get(v.index()).copied())
    }
}

/// Converts formulas to CNF with the Tseitin transformation, and collects
/// the theory atoms and their defining rows.
struct Encoder {
    n_real: usize,
    next_var: u32,
    clauses: Vec<Vec<Lit>>,
    /// Rows defining each distinct expression as a slack variable.
    rows: Vec<Vec<(usize, Rational)>>,
    slack_of: BTreeMap<Vec<(Var, Rational)>, usize>,
    atoms: Vec<TheoryAtom>,
    memo: BTreeMap<u32, Lit>,
}

impl Encoder {
    fn new(n_real: usize) -> Self {
        Self {
            n_real,
            // Variable 0 is the constant `true`.
            next_var: 1,
            clauses: Vec::new(),
            rows: Vec::new(),
            slack_of: BTreeMap::new(),
            atoms: Vec::new(),
            memo: BTreeMap::new(),
        }
    }

    fn fresh(&mut self) -> Lit {
        let v = self.next_var;
        self.next_var += 1;
        Lit::new(v, false)
    }

    fn encode(&mut self, arena: &Arena, f: Formula) -> Result<Lit, Unknown> {
        if let Some(&l) = self.memo.get(&f.0) {
            return Ok(l);
        }
        let lit = match arena.node(f).clone() {
            Node::Const(b) => {
                let t = Lit::new(0, false);
                if b { t } else { t.negate() }
            }
            Node::Atom(atom) => self.atom(atom.terms, atom.bound, atom.strict)?,
            Node::Not(g) => self.encode(arena, g)?.negate(),
            Node::And(gs) => {
                let lits = self.encode_all(arena, &gs)?;
                self.and_gate(&lits)
            }
            Node::Or(gs) => {
                let lits = self.encode_all(arena, &gs)?;
                self.or_gate(&lits)
            }
        };
        self.memo.insert(f.0, lit);
        Ok(lit)
    }

    fn encode_all(&mut self, arena: &Arena, gs: &[Formula]) -> Result<Vec<Lit>, Unknown> {
        let mut out = Vec::with_capacity(gs.len());
        for &g in gs {
            out.push(self.encode(arena, g)?);
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
    fn atom(
        &mut self,
        terms: Vec<(Var, Rational)>,
        bound: Rational,
        strict: bool,
    ) -> Result<Lit, Unknown> {
        if terms.is_empty() {
            // 0 <= bound, or 0 < bound.
            let holds = if strict {
                !bound.is_negative() && !bound.is_zero()
            } else {
                !bound.is_negative()
            };
            let t = Lit::new(0, false);
            return Ok(if holds { t } else { t.negate() });
        }
        let slack = match self.slack_of.get(&terms) {
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
            slack: self.n_real + slack,
            bound,
            strict,
            var,
        });
        Ok(Lit::new(var, false))
    }
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
}

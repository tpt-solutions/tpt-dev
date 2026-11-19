//! Proofs of unsatisfiability for Boolean-structured formulas.
//!
//! # Format
//!
//! A [`Proof`] is a list of steps. Clauses use DIMACS numbering: literal `k`
//! is Boolean variable `k` true, and `-k` is it false. Variables are numbered
//! by the crate's Tseitin encoder, and variable `1` is the constant `true`.
//!
//! The *base clauses* are the Tseitin clauses of the assertions, the unit
//! `[1]` for the constant, and one unit for each assertion. They are not part
//! of the proof. The checker rebuilds them from the formulas, so the producer
//! cannot change what is being refuted.
//!
//! Each atom formula has its own Boolean variable. The literal `(f, true)`
//! means "atom `f` holds", and `(f, false)` means "atom `f` does not hold".
//!
//! The steps run in order, and each adds one clause to the database:
//!
//! - [`ProofStep::Rup`] adds a clause `C` that follows by reverse unit
//!   propagation. Assigning every literal of `C` false and propagating over
//!   the base clauses and the earlier steps must reach a conflict.
//! - [`ProofStep::Theory`] adds the clause `not (l_1 and ... and l_k)`, where
//!   `l_i` is atom `f_i` if its flag is `true` and `not f_i` otherwise. The
//!   certificate must pass the [`Farkas`] rules for those literals, so the
//!   conjunction is unsatisfiable in linear arithmetic.
//!
//! A proof of unsatisfiability has a final step `Rup([])`, which is the empty
//! clause. [`check_proof`] accepts a proof only when every step is valid and
//! the last step is the empty clause. It never calls the solver.
//!
//! The trusted parts are the Tseitin encoder, unit propagation in the
//! checker, and [`check_farkas`]'s rules. The SAT search, the simplex and
//! conflict analysis are not trusted: the producer records only what the
//! checker re-verifies.
//!
//! [`check_farkas`]: crate::check_farkas

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use crate::Budget;
use crate::Unknown;
use crate::rational::Rational;
use crate::sat::{Lit, Sat, SatResult};
use crate::simplex::Simplex;
use crate::solver::{
    Encoder, Farkas, TheoryAtom, TheoryOutcome, farkas_conflict, theory_check, verify_farkas,
};
use crate::term::{Arena, Formula, Source};

/// One step of a [`Proof`]. See the module docs for the rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProofStep {
    /// A clause that follows from the clauses before it by reverse unit
    /// propagation. Literals are DIMACS numbers.
    Rup(Vec<i32>),
    /// A theory lemma: the negation of the conjunction of `lits`, justified by
    /// `cert`. The atoms in `lits` must be encoded in the proof's problem.
    Theory {
        /// `(atom, positive)`. A `false` flag means the atom must not hold.
        lits: Vec<(Formula, bool)>,
        /// A certificate that `lits` is unsatisfiable.
        cert: Farkas,
    },
}

/// A checkable proof that a set of assertions is unsatisfiable. See the
/// module docs for the format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    steps: Vec<ProofStep>,
}

impl Proof {
    /// Builds a proof from its steps. Nothing is checked until [`check_proof`].
    pub fn new(steps: Vec<ProofStep>) -> Self {
        Self { steps }
    }

    /// The steps, in order.
    pub fn steps(&self) -> &[ProofStep] {
        &self.steps
    }
}

/// The base clauses of a set of assertions, built by the Tseitin encoder.
struct Base {
    clauses: Vec<Vec<Lit>>,
    n_vars: usize,
    rows: Vec<Vec<(usize, Rational)>>,
    atoms: Vec<TheoryAtom>,
    /// Boolean variable of each encoded atom, keyed by formula node.
    atom_var: BTreeMap<u32, u32>,
}

impl Base {
    fn new<S: Source>(src: &S, roots: &[Formula], max_nodes: usize) -> Result<Self, Unknown> {
        let mut enc = Encoder::new();
        // Variable 0 is the constant true.
        enc.clauses.push(vec![Lit::new(0, false)]);
        let mut root_lits = Vec::with_capacity(roots.len());
        for &f in roots {
            root_lits.push(enc.encode(src, f, max_nodes)?);
        }
        for l in root_lits {
            enc.clauses.push(vec![l]);
        }
        let mut clauses = enc.clauses;
        for c in &mut clauses {
            c.sort_unstable_by_key(|l| l.0);
            c.dedup();
        }
        let atom_var = enc.atom_vars.iter().map(|&(v, f)| (f.0, v)).collect();
        Ok(Self {
            clauses,
            n_vars: enc.next_var as usize,
            rows: enc.rows,
            atoms: enc.atoms,
            atom_var,
        })
    }
}

fn to_dimacs(l: Lit) -> i32 {
    let d = l.var() as i32 + 1;
    if l.is_negative() { -d } else { d }
}

/// Converts DIMACS literals to internal ones, or `None` if one is out of range.
fn from_dimacs(clause: &[i32], n_vars: usize) -> Option<Vec<Lit>> {
    let mut out = Vec::with_capacity(clause.len());
    for &d in clause {
        if d == 0 {
            return None;
        }
        let v = d.unsigned_abs() as usize - 1;
        if v >= n_vars {
            return None;
        }
        out.push(Lit::new(v as u32, d < 0));
    }
    out.sort_unstable_by_key(|l| l.0);
    out.dedup();
    Some(out)
}

/// Literal value under `val`: 1 true, -1 false, 0 unassigned.
fn lit_value(val: &[i8], l: Lit) -> i8 {
    let v = val[l.var()];
    if l.is_negative() { -v } else { v }
}

/// Whether `clause` follows from `db` by reverse unit propagation.
fn is_rup(db: &[Vec<Lit>], clause: &[Lit], n_vars: usize) -> bool {
    let mut val = vec![0i8; n_vars];
    for &l in clause {
        // Make `l` false.
        let want = if l.is_negative() { 1 } else { -1 };
        match lit_value(&val, l) {
            0 => val[l.var()] = want,
            // Already false, as assumed.
            -1 => {}
            // The clause is a tautology, so it holds.
            _ => return true,
        }
    }
    loop {
        let mut changed = false;
        for c in db {
            let mut unassigned = None;
            let mut count = 0usize;
            let mut satisfied = false;
            for &l in c {
                match lit_value(&val, l) {
                    1 => {
                        satisfied = true;
                        break;
                    }
                    0 => {
                        count += 1;
                        unassigned = Some(l);
                    }
                    _ => {}
                }
            }
            if satisfied {
                continue;
            }
            match (count, unassigned) {
                (0, _) => return true,
                (1, Some(l)) => {
                    val[l.var()] = if l.is_negative() { -1 } else { 1 };
                    changed = true;
                }
                _ => {}
            }
        }
        if !changed {
            return false;
        }
    }
}

/// Produces a proof that `roots` are unsatisfiable, or `Ok(None)` if they
/// are satisfiable. Runs the CDCL search with theory checks, and records each
/// clause it learns. Theory clauses get a Farkas certificate. The caller
/// should check the result with [`check_proof`].
pub(crate) fn produce<S: Source>(
    src: &S,
    roots: &[Formula],
    budget: &Budget,
) -> Result<Option<Proof>, Unknown> {
    let mut base = Base::new(src, roots, budget.max_formula_nodes)?;
    let mut sat = Sat::new();
    sat.ensure_vars(base.n_vars);
    sat.set_max_clauses(budget.max_clauses);
    sat.start_trace();
    for c in core::mem::take(&mut base.clauses) {
        sat.add_clause(c);
    }
    let n = src.var_count();
    let mut simplex = Simplex::new(n, &base.rows, budget.max_pivots)?;
    simplex.start_budget(budget.max_pivots);
    let mut theory = |vals: &[i8]| -> Result<Option<Vec<Lit>>, Unknown> {
        match theory_check(&mut simplex, n, &base.atoms, vals)? {
            TheoryOutcome::Conflict(clause) => Ok(Some(clause)),
            TheoryOutcome::Sat(_) => Ok(None),
        }
    };
    if sat.solve(&[], &mut theory)? == SatResult::Sat {
        return Ok(None);
    }
    let atom_of: BTreeMap<u32, Formula> = base
        .atom_var
        .iter()
        .map(|(&f, &v)| (v, Formula(f)))
        .collect();
    let mut steps = Vec::new();
    for (clause, is_theory) in sat.take_trace() {
        if is_theory {
            // A theory clause `not (l_1 and ...)` has literals `l_i` of the
            // form (var, negated). The atom holds exactly when its literal in
            // the clause is negative.
            let mut lits = Vec::with_capacity(clause.len());
            for &c in &clause {
                let f = *atom_of.get(&(c.var() as u32)).ok_or(Unknown)?;
                lits.push((f, c.is_negative()));
            }
            let cert = farkas_conflict(src, &lits, budget.max_pivots)?.ok_or(Unknown)?;
            steps.push(ProofStep::Theory { lits, cert });
        } else {
            steps.push(ProofStep::Rup(
                clause.iter().map(|&l| to_dimacs(l)).collect(),
            ));
        }
    }
    steps.push(ProofStep::Rup(Vec::new()));
    Ok(Some(Proof { steps }))
}

/// Checks `proof` against `assertions`, without the solver.
///
/// Returns `true` only if every step is valid, as described in the module
/// docs, and the last step is the empty clause. The proof must have been
/// built from formulas in `arena`. Atom formulas from another arena may
/// panic.
///
/// ```
/// use tpt_dev_solver::{check_proof, Linear, Rational, Solver};
///
/// let mut s = Solver::new();
/// let x = s.declare("x");
/// let k = |n: i128| Linear::constant(Rational::from_int(n));
/// // (x <= 0 or x >= 10) and x > 5 and x < 9 is unsatisfiable.
/// let a = s.arena_mut().le(&Linear::var(x), &k(0)).unwrap();
/// let b = s.arena_mut().ge(&Linear::var(x), &k(10)).unwrap();
/// let or = s.arena_mut().or(vec![a, b]);
/// let c = s.arena_mut().gt(&Linear::var(x), &k(5)).unwrap();
/// let d = s.arena_mut().lt(&Linear::var(x), &k(9)).unwrap();
/// for f in [or, c, d] {
///     s.assert(f);
/// }
/// let proof = s.unsat_proof().unwrap().unwrap();
/// assert!(check_proof(s.arena(), s.assertions(), &proof));
/// ```
pub fn check_proof(arena: &Arena, assertions: &[Formula], proof: &Proof) -> bool {
    let Ok(base) = Base::new(arena, assertions, usize::MAX) else {
        return false;
    };
    let mut db = base.clauses.clone();
    for step in &proof.steps {
        match step {
            ProofStep::Theory { lits, cert } => {
                let mut clause = Vec::with_capacity(lits.len());
                for &(f, positive) in lits {
                    let Some(&v) = base.atom_var.get(&f.0) else {
                        return false;
                    };
                    // The negation of "atom holds iff positive".
                    clause.push(Lit::new(v, positive));
                }
                if !verify_farkas(arena, lits, cert) {
                    return false;
                }
                clause.sort_unstable_by_key(|l| l.0);
                clause.dedup();
                db.push(clause);
            }
            ProofStep::Rup(d) => {
                let Some(clause) = from_dimacs(d, base.n_vars) else {
                    return false;
                };
                if !is_rup(&db, &clause, base.n_vars) {
                    return false;
                }
                db.push(clause);
            }
        }
    }
    matches!(proof.steps.last(), Some(ProofStep::Rup(d)) if d.is_empty())
}

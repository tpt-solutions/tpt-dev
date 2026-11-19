//! Invariant generation: candidate linear bounds inferred from samples, then
//! proved or refuted by the solver.

use alloc::vec::Vec;

use tpt_dev_solver::{Arena, Formula, Linear, Rational, Solver, Status, Unknown, Var};

/// A candidate linear bound `sum(c_i * x_i) <= bound` over the variables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// Nonzero coefficients, as `(variable index, coefficient)`.
    pub coeffs: Vec<(usize, i64)>,
    /// The bound on the sum.
    pub bound: Rational,
}

impl Candidate {
    /// The bound as a solver formula over `vars`.
    pub fn to_formula(&self, arena: &mut Arena, vars: &[Var]) -> Option<Formula> {
        let lhs = self.lhs(vars)?;
        let rhs = Linear::constant(self.bound);
        arena.le(&lhs, &rhs)
    }

    fn lhs(&self, vars: &[Var]) -> Option<Linear> {
        let mut lhs = Linear::constant(Rational::ZERO);
        for &(i, c) in &self.coeffs {
            let v = *vars.get(i)?;
            let term = Linear::var(v).scale(Rational::from_int(i128::from(c)))?;
            lhs = lhs.checked_add(&term)?;
        }
        Some(lhs)
    }

    /// The value of the sum at `point`, or `None` on overflow.
    pub(crate) fn at(&self, point: &[Rational]) -> Option<Rational> {
        let mut acc = Rational::ZERO;
        for &(i, c) in &self.coeffs {
            let term = point
                .get(i)?
                .checked_mul(Rational::from_int(i128::from(c)))?;
            acc = acc.checked_add(term)?;
        }
        Some(acc)
    }
}

/// Candidate directions: single variables, and pairs with each sign pattern.
fn directions(n: usize) -> Vec<Vec<(usize, i64)>> {
    let mut out = Vec::new();
    for i in 0..n {
        out.push(alloc::vec![(i, 1)]);
        out.push(alloc::vec![(i, -1)]);
    }
    for i in 0..n {
        for j in (i + 1)..n {
            for (a, b) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                out.push(alloc::vec![(i, a), (j, b)]);
            }
        }
    }
    out
}

/// Infers candidate bounds from `samples`, where each sample has `n` values.
///
/// For each direction (single variables, and pairs of variables with each
/// sign pattern), the bound is the largest value the sum takes in the samples.
/// These are only candidates. The samples do not prove anything, so each one
/// must be checked with [`prove`].
///
/// ```
/// use tpt_dev_test::infer;
/// use tpt_dev_solver::Rational;
///
/// let samples = vec![
///     vec![Rational::from_int(3), Rational::from_int(7)],
///     vec![Rational::from_int(4), Rational::from_int(6)],
/// ];
/// let candidates = infer(&samples, 2);
/// // x + y is always 10 in these samples, so `x + y <= 10` is a candidate.
/// assert!(candidates.iter().any(|c| c.coeffs == vec![(0, 1), (1, 1)] && c.bound == Rational::from_int(10)));
/// ```
pub fn infer(samples: &[Vec<Rational>], n: usize) -> Vec<Candidate> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for dir in directions(n) {
        let cand = Candidate {
            coeffs: dir,
            bound: Rational::ZERO,
        };
        let mut best: Option<Rational> = None;
        let mut overflow = false;
        for s in samples {
            match cand.at(s) {
                Some(v) => {
                    best = Some(match best {
                        Some(b) if b >= v => b,
                        _ => v,
                    });
                }
                None => {
                    overflow = true;
                    break;
                }
            }
        }
        if let (false, Some(bound)) = (overflow, best) {
            out.push(Candidate { bound, ..cand });
        }
    }
    out
}

/// Whether a candidate holds for every model of a precondition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The bound holds in every model of the precondition.
    Holds,
    /// A model of the precondition breaks the bound. `witness` gives its values.
    Refuted {
        /// One value per variable, in declaration order.
        witness: Vec<Rational>,
    },
}

/// Decides whether `pre` implies `candidate`.
///
/// The solver checks `pre and not candidate`. Unsat means the candidate is a
/// proven invariant of the precondition. Sat gives a counterexample. The
/// solver's assertions are restored afterwards.
pub fn prove(
    solver: &mut Solver,
    vars: &[Var],
    pre: Formula,
    candidate: &Candidate,
) -> Result<Verdict, Unknown> {
    let arena = solver.arena_mut();
    let holds = candidate.to_formula(arena, vars).ok_or(Unknown)?;
    let violated = arena.not(holds);
    solver.push();
    solver.assert(pre);
    solver.assert(violated);
    let result = solver.check();
    let verdict = match result {
        Ok(Status::Unsat) => Ok(Verdict::Holds),
        Ok(Status::Sat) => Ok(Verdict::Refuted {
            witness: vars
                .iter()
                .map(|&v| solver.value(v).unwrap_or(Rational::ZERO))
                .collect(),
        }),
        Err(e) => Err(e),
    };
    solver.pop();
    verdict
}

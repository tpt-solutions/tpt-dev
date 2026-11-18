//! Generators whose values satisfy a solver formula.

use alloc::vec::Vec;
use core::cell::RefCell;

use tpt_dev_solver::{Arena, Formula, Linear, Rational, Solver, Status, Unknown, Var};

use crate::generator::Generator;
use crate::rng::Rng;

/// Why a [`ConstrainedGen`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupError {
    /// The constraint has no solutions, so there is nothing to generate.
    Unsatisfiable,
    /// The solver could not decide the constraint.
    Unknown,
}

/// Generates assignments of real variables that satisfy a formula.
///
/// Each call adds random lower bounds to a random subset of the variables, and
/// keeps them only while the formula stays satisfiable. The solver then gives
/// a model, so every value satisfies the formula, and the random bounds spread
/// the models out. Each value is a `Vec<Rational>`, one entry per variable.
///
/// ```
/// use tpt_dev_test::{ConstrainedGen, Generator, Rng, Seed};
/// use tpt_dev_solver::{Linear, Rational};
///
/// // x + y = 10 and x >= 0 and y >= 0.
/// let generator = ConstrainedGen::new(&["x", "y"], |arena, v| {
///     let sum = Linear::var(v[0]).checked_add(&Linear::var(v[1])).unwrap();
///     let ten = Linear::constant(Rational::from_int(10));
///     let zero = Linear::constant(Rational::ZERO);
///     let eq = arena.eq(&sum, &ten)?;
///     let x_ok = arena.ge(&Linear::var(v[0]), &zero)?;
///     let y_ok = arena.ge(&Linear::var(v[1]), &zero)?;
///     Some(arena.and(vec![eq, x_ok, y_ok]))
/// }).unwrap();
/// let sample = generator.generate(&mut Rng::new(Seed(1)), 10);
/// assert_eq!(sample[0].checked_add(sample[1]).unwrap(), Rational::from_int(10));
/// ```
pub struct ConstrainedGen {
    solver: RefCell<Solver>,
    vars: Vec<Var>,
    spread: i64,
    fallback: Vec<Rational>,
}

impl ConstrainedGen {
    /// Declares one real variable per name, and builds the constraint from them.
    ///
    /// The builder returns `None` if a formula overflows the solver's arithmetic.
    pub fn new<F>(names: &[&str], build: F) -> Result<Self, SetupError>
    where
        F: FnOnce(&mut Arena, &[Var]) -> Option<Formula>,
    {
        let mut solver = Solver::new();
        let vars: Vec<Var> = names.iter().map(|n| solver.declare(n)).collect();
        let base = build(solver.arena_mut(), &vars).ok_or(SetupError::Unknown)?;
        solver.assert(base);
        match solver.check() {
            Ok(Status::Sat) => {}
            Ok(Status::Unsat) => return Err(SetupError::Unsatisfiable),
            Err(Unknown) => return Err(SetupError::Unknown),
        }
        let fallback = vars
            .iter()
            .map(|&v| solver.value(v).unwrap_or(Rational::ZERO))
            .collect();
        Ok(Self {
            solver: RefCell::new(solver),
            vars,
            spread: 10,
            fallback,
        })
    }

    /// Sets the largest magnitude of the random lower bounds (default 10).
    pub fn with_spread(mut self, spread: i64) -> Self {
        self.spread = spread.max(0);
        self
    }
}

impl Generator for ConstrainedGen {
    type Item = Vec<Rational>;

    fn generate(&self, rng: &mut Rng, _size: usize) -> Vec<Rational> {
        let mut solver = self.solver.borrow_mut();
        solver.push();
        let mut bounds = Vec::new();
        for &v in &self.vars {
            if rng.bool() {
                let r = rng.range_i64(-self.spread, self.spread);
                let lhs = Linear::var(v);
                let c = Linear::constant(Rational::from_int(i128::from(r)));
                if let Some(f) = solver.arena_mut().ge(&lhs, &c) {
                    bounds.push(f);
                }
            }
        }
        // Keep only the bounds that leave the formula satisfiable.
        let mut kept = Vec::new();
        for f in bounds {
            solver.assert(f);
            match solver.check() {
                Ok(Status::Sat) => kept.push(f),
                _ => {
                    solver.pop();
                    solver.push();
                    for &g in &kept {
                        solver.assert(g);
                    }
                }
            }
        }
        let sample = match solver.check() {
            Ok(Status::Sat) => self
                .vars
                .iter()
                .map(|&v| solver.value(v).unwrap_or(Rational::ZERO))
                .collect(),
            _ => self.fallback.clone(),
        };
        solver.pop();
        sample
    }
}

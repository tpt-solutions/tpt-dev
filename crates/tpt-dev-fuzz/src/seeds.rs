//! Solver-derived seeds: inputs that satisfy a linear precondition.

use alloc::vec::Vec;

use tpt_dev_solver::{Arena, Formula, Var};
use tpt_dev_test::{ConstrainedGen, Generator, Rng, Seed};

/// Why no seeds could be produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedError {
    /// The precondition has no real solutions.
    Unsatisfiable,
    /// The solver could not decide the precondition.
    Unknown,
    /// Solutions exist, but none had integer values for every input.
    NoIntegerSolutions,
}

/// Up to `count` integer inputs that satisfy a linear precondition.
///
/// The solver works over the reals, so its models can be fractional. Inputs
/// are integers, so only models with integer values for every input are kept.
/// Samples are drawn from the solver's models by [`ConstrainedGen`], which
/// spreads them out. Inputs that are not integral are discarded. An
/// equality such as `a + b = 7` often has integral vertex solutions, but not always.
///
/// ```
/// use tpt_dev_fuzz::solver_seeds;
/// use tpt_dev_solver::{Linear, Rational};
///
/// // a + b = 7 and a >= 3
/// let seeds = solver_seeds(2, 8, |arena, v| {
///     let sum = Linear::var(v[0]).checked_add(&Linear::var(v[1]))?;
///     let eq = arena.eq(&sum, &Linear::constant(Rational::from_int(7)))?;
///     let lo = arena.ge(&Linear::var(v[0]), &Linear::constant(Rational::from_int(3)))?;
///     Some(arena.and(vec![eq, lo]))
/// }).unwrap();
/// assert!(seeds.iter().all(|s| s[0] + s[1] == 7 && s[0] >= 3));
/// ```
pub fn solver_seeds<F>(arity: usize, count: usize, build: F) -> Result<Vec<Vec<i64>>, SeedError>
where
    F: FnOnce(&mut Arena, &[Var]) -> Option<Formula>,
{
    let names: Vec<alloc::string::String> = (0..arity).map(|i| alloc::format!("x{i}")).collect();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let sampler = ConstrainedGen::new(&refs, build).map_err(|e| match e {
        tpt_dev_test::SetupError::Unsatisfiable => SeedError::Unsatisfiable,
        tpt_dev_test::SetupError::Unknown => SeedError::Unknown,
    })?;
    let mut rng = Rng::new(Seed(0xF00D));
    let mut out = Vec::new();
    // Models are sampled until enough integral ones are found, with a bounded number of tries.
    for _ in 0..count.saturating_mul(50) {
        if out.len() >= count {
            break;
        }
        let sample = sampler.generate(&mut rng, arity);
        let integral: Option<Vec<i64>> = sample
            .iter()
            .map(|r| {
                if r.is_integer() {
                    i64::try_from(r.num()).ok()
                } else {
                    None
                }
            })
            .collect();
        if let Some(v) = integral
            && !out.contains(&v)
        {
            out.push(v);
        }
    }
    if out.is_empty() {
        return Err(SeedError::NoIntegerSolutions);
    }
    Ok(out)
}

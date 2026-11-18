//! Running a property over generated cases, shrinking failures, and replaying them.

use crate::generator::Generator;
use crate::rng::{Rng, Seed, case_seed};

/// How many cases to run, from which seed, and how hard to shrink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Number of cases.
    pub cases: u32,
    /// Seed for the whole run. Recording it is enough to reproduce every case.
    pub seed: Seed,
    /// Upper bound on the size passed to generators.
    pub max_size: usize,
    /// Maximum number of property evaluations spent shrinking one failure.
    pub max_shrinks: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cases: 100,
            seed: Seed(0x5EED_0001),
            max_size: 50,
            max_shrinks: 2000,
        }
    }
}

/// A failing case: where it came from, and the smallest input found that still fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure<T> {
    /// The run's seed. With `case`, this reproduces the failure exactly.
    pub seed: Seed,
    /// Index of the failing case.
    pub case: u32,
    /// The input as generated.
    pub original: T,
    /// The smallest failing input found by shrinking.
    pub minimal: T,
    /// Number of successful shrink steps.
    pub shrinks: u32,
}

fn size_for(config: &Config, case: u32) -> usize {
    // Early cases are small, and later cases reach the maximum size.
    (case as usize + 1).min(config.max_size)
}

/// The input generated for case `case` of a run. Use it to inspect or rerun one failing case.
///
/// ```
/// use tpt_dev_test::{Config, IntRange, replay};
///
/// let config = Config::default();
/// let generator = IntRange::new(0, 100);
/// // The same seed and case index always give the same input.
/// assert_eq!(replay(&config, 3, &generator), replay(&config, 3, &generator));
/// ```
pub fn replay<G: Generator>(config: &Config, case: u32, generator: &G) -> G::Item {
    let mut rng = Rng::new(case_seed(config.seed, case));
    generator.generate(&mut rng, size_for(config, case))
}

/// Checks `prop` on `config.cases` generated inputs.
///
/// Returns `Ok(())` if the property holds on all of them. Otherwise it shrinks
/// the first failing input and returns a [`Failure`].
///
/// ```
/// use tpt_dev_test::{Config, IntRange, check};
///
/// let config = Config::default();
/// let result = check(&config, &IntRange::new(0, 1000), |&x| x < 100);
/// let failure = result.unwrap_err();
/// // Shrinking finds the smallest counterexample, 100.
/// assert_eq!(failure.minimal, 100);
/// ```
pub fn check<G, P>(config: &Config, generator: &G, prop: P) -> Result<(), Failure<G::Item>>
where
    G: Generator,
    P: Fn(&G::Item) -> bool,
{
    for case in 0..config.cases {
        let original = replay(config, case, generator);
        if prop(&original) {
            continue;
        }
        let (minimal, shrinks) = shrink(config, generator, &prop, original.clone());
        return Err(Failure {
            seed: config.seed,
            case,
            original,
            minimal,
            shrinks,
        });
    }
    Ok(())
}

/// Greedy shrinking. Takes the first candidate that still fails, and repeats
/// until none does or the budget runs out.
fn shrink<G, P>(config: &Config, generator: &G, prop: &P, mut current: G::Item) -> (G::Item, u32)
where
    G: Generator,
    P: Fn(&G::Item) -> bool,
{
    let mut budget = config.max_shrinks;
    let mut steps = 0;
    loop {
        // Try the candidates for the current value. Accept the first that still
        // fails, and start again from it.
        let mut accepted = None;
        for candidate in generator.shrink(&current) {
            if budget == 0 {
                return (current, steps);
            }
            budget -= 1;
            if !prop(&candidate) {
                accepted = Some(candidate);
                break;
            }
        }
        match accepted {
            Some(next) => {
                current = next;
                steps += 1;
            }
            None => return (current, steps),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::generator::{IntRange, VecOf};
    use alloc::vec::Vec;

    #[test]
    fn passing_property_is_ok() {
        let config = Config::default();
        assert!(check(&config, &IntRange::new(0, 10), |&x| (0..=10).contains(&x)).is_ok());
    }

    #[test]
    fn failure_shrinks_to_boundary() {
        let config = Config {
            seed: Seed(99),
            ..Config::default()
        };
        let failure = check(&config, &IntRange::new(0, 5000), |&x| x < 1234).unwrap_err();
        assert_eq!(failure.minimal, 1234);
        assert!(failure.original >= 1234);
        assert!(failure.shrinks > 0);
    }

    #[test]
    fn failures_are_reproducible_from_seed_and_case() {
        let config = Config {
            seed: Seed(5),
            ..Config::default()
        };
        let generator = VecOf::new(IntRange::new(-50, 50), 20);
        let prop = |v: &Vec<i64>| v.iter().sum::<i64>() < 100;
        let failure = check(&config, &generator, prop).unwrap_err();
        let again = replay(&config, failure.case, &generator);
        assert_eq!(again, failure.original);
        assert!(!prop(&failure.minimal));
    }

    #[test]
    fn shrink_budget_is_respected() {
        let config = Config {
            max_shrinks: 3,
            ..Config::default()
        };
        let failure = check(&config, &IntRange::new(0, 1_000_000), |&x| x < 500_000).unwrap_err();
        assert!(failure.shrinks <= 3);
        assert!(!(failure.minimal < 500_000));
    }
}

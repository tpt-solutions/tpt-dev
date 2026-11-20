//! The coverage-guided mutation loop, and crash minimisation.

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

use tpt_dev_test::{Rng, Seed};

use crate::coverage::Coverage;

/// What a target run reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The input was handled.
    Pass,
    /// The input triggers a bug. The message describes it.
    Crash(String),
}

/// How hard to fuzz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Number of mutated inputs to try after the seeds.
    pub iterations: u64,
    /// Seed for the mutation choices. The same seed gives the same run.
    pub seed: Seed,
    /// Smallest value a mutated input coordinate may take.
    pub min: i64,
    /// Largest value a mutated input coordinate may take.
    pub max: i64,
    /// Largest corpus size. Above it, a random entry is replaced.
    pub corpus_cap: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            iterations: 10_000,
            seed: Seed(0xF022),
            min: -10_000,
            max: 10_000,
            corpus_cap: 512,
        }
    }
}

/// A crash found by the fuzzer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The crashing input, after minimisation.
    pub input: Vec<i64>,
    /// The message the target reported.
    pub message: String,
    /// Mutated inputs tried before the crash. Seeds do not count.
    pub iterations: u64,
}

/// Coverage-guided fuzzer over fixed-length integer inputs.
///
/// Inputs that reach a new edge join the corpus, and later mutations start
/// from corpus entries. The target gets a fresh [`Coverage`] on every run.
///
/// ```
/// use tpt_dev_fuzz::{Config, Coverage, Fuzzer, Outcome};
///
/// let target = |v: &[i64], cov: &mut Coverage| {
///     cov.hit(0);
///     if v[0] == 42 {
///         cov.hit(1);
///         return Outcome::Crash(String::from("hit 42"));
///     }
///     Outcome::Pass
/// };
/// let mut fuzzer = Fuzzer::new(target, 1, Config { iterations: 50_000, ..Config::default() });
/// let found = fuzzer.run(&[]).expect("should find 42");
/// assert_eq!(found.input, vec![42]);
/// ```
pub struct Fuzzer<T> {
    target: T,
    arity: usize,
    config: Config,
    corpus: Vec<Vec<i64>>,
    seen: BTreeSet<u32>,
    rng: Rng,
}

impl<T> Fuzzer<T>
where
    T: FnMut(&[i64], &mut Coverage) -> Outcome,
{
    /// A fuzzer for inputs of `arity` integers.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_fuzz::{Config, Coverage, Fuzzer, Outcome};
    ///
    /// let target = |_input: &[i64], _cov: &mut Coverage| Outcome::Pass;
    /// let fuzzer = Fuzzer::new(target, 2, Config::default());
    /// drop(fuzzer);
    /// ```
    pub fn new(target: T, arity: usize, config: Config) -> Self {
        Self {
            target,
            arity,
            config,
            corpus: Vec::new(),
            seen: BTreeSet::new(),
            rng: Rng::new(config.seed),
        }
    }

    /// Runs the fuzzer. `seeds` start the corpus; seeds of the wrong length are ignored.
    ///
    /// Returns the first crash found, minimised, or `None` if the budget runs out.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_fuzz::{Config, Coverage, Fuzzer, Outcome};
    ///
    /// // The bug is any first coordinate above 5.
    /// let target = |input: &[i64], _cov: &mut Coverage| {
    ///     if input[0] > 5 {
    ///         Outcome::Crash(format!("x = {}", input[0]))
    ///     } else {
    ///         Outcome::Pass
    ///     }
    /// };
    /// let mut fuzzer = Fuzzer::new(target, 1, Config { iterations: 1000, ..Config::default() });
    /// let found = fuzzer.run(&[vec![0]]).expect("the fuzzer should find the bug");
    /// assert!(found.input[0] > 5);
    /// ```
    pub fn run(&mut self, seeds: &[Vec<i64>]) -> Option<Found> {
        if self.arity == 0 {
            return None;
        }
        let mut starts: Vec<Vec<i64>> = seeds
            .iter()
            .filter(|s| s.len() == self.arity)
            .cloned()
            .collect();
        if starts.is_empty() {
            starts.push(alloc::vec![0; self.arity]);
        }
        for s in starts {
            if let Some(found) = self.execute(&s, 0) {
                return Some(found);
            }
        }
        for iteration in 1..=self.config.iterations {
            let parent = self.pick_parent();
            let child = self.mutate(&parent);
            if let Some(found) = self.execute(&child, iteration) {
                return Some(found);
            }
        }
        None
    }

    /// Runs the target. Keeps the input if it reaches a new edge. Returns a
    /// minimised crash if the input crashes.
    fn execute(&mut self, input: &[i64], iteration: u64) -> Option<Found> {
        let mut cov = Coverage::new();
        let outcome = (self.target)(input, &mut cov);
        let mut new_edge = false;
        for e in cov.edges() {
            if self.seen.insert(e) {
                new_edge = true;
            }
        }
        match outcome {
            Outcome::Crash(message) => {
                let (input, message) =
                    minimise(&mut self.target, input).unwrap_or((input.to_vec(), message));
                Some(Found {
                    input,
                    message,
                    iterations: iteration,
                })
            }
            Outcome::Pass => {
                if new_edge {
                    self.keep(input.to_vec());
                }
                None
            }
        }
    }

    fn keep(&mut self, input: Vec<i64>) {
        if self.corpus.len() >= self.config.corpus_cap && !self.corpus.is_empty() {
            let i = self.rng.range_i64(0, self.corpus.len() as i64 - 1) as usize;
            self.corpus[i] = input;
        } else {
            self.corpus.push(input);
        }
    }

    fn pick_parent(&mut self) -> Vec<i64> {
        if self.corpus.is_empty() {
            return alloc::vec![0; self.arity];
        }
        let i = self.rng.range_i64(0, self.corpus.len() as i64 - 1) as usize;
        self.corpus[i].clone()
    }

    /// Applies one to three random mutations to a copy of `parent`.
    fn mutate(&mut self, parent: &[i64]) -> Vec<i64> {
        let mut child = parent.to_vec();
        let ops = self.rng.range_i64(1, 3);
        for _ in 0..ops {
            let i = self.rng.range_i64(0, self.arity as i64 - 1) as usize;
            match self.rng.range_i64(0, 3) {
                0 => {
                    // Small random step.
                    let d = self.rng.range_i64(-64, 64);
                    child[i] = child[i].saturating_add(d);
                }
                1 => {
                    // Uniform value in the configured range.
                    child[i] = self.rng.range_i64(self.config.min, self.config.max);
                }
                2 => {
                    // Copy a coordinate from another corpus entry.
                    if !self.corpus.is_empty() {
                        let k = self.rng.range_i64(0, self.corpus.len() as i64 - 1) as usize;
                        if let Some(&v) = self.corpus[k].get(i) {
                            child[i] = v;
                        }
                    }
                }
                _ => {
                    // Swap two coordinates.
                    let j = self.rng.range_i64(0, self.arity as i64 - 1) as usize;
                    child.swap(i, j);
                }
            }
            child[i] = child[i].clamp(self.config.min, self.config.max);
        }
        child
    }
}

/// Shrinks a crashing input while it still crashes. Returns the smaller input and its message,
/// or `None` if `input` does not crash.
///
/// Each coordinate moves toward zero by halving, and then by single steps,
/// as long as the target still crashes.
///
/// # Examples
///
/// ```
/// use tpt_dev_fuzz::{Coverage, Outcome, minimise};
///
/// let mut target = |input: &[i64], _cov: &mut Coverage| {
///     if input[0] > 5 {
///         Outcome::Crash("too big".to_string())
///     } else {
///         Outcome::Pass
///     }
/// };
/// let (small, message) = minimise(&mut target, &[900]).unwrap();
/// assert_eq!(message, "too big");
/// assert!(small[0] > 5);
/// assert!(minimise(&mut target, &[3]).is_none());
/// ```
pub fn minimise<T>(target: &mut T, input: &[i64]) -> Option<(Vec<i64>, String)>
where
    T: FnMut(&[i64], &mut Coverage) -> Outcome,
{
    let mut current = input.to_vec();
    let mut message = match target(&current, &mut Coverage::new()) {
        Outcome::Crash(m) => m,
        Outcome::Pass => return None,
    };
    let mut progress = true;
    while progress {
        progress = false;
        for i in 0..current.len() {
            loop {
                let x = current[i];
                if x == 0 {
                    break;
                }
                // Try zero, then halving, then one step toward zero.
                let candidates = [0, x / 2, if x > 0 { x - 1 } else { x + 1 }];
                let mut moved = false;
                for c in candidates {
                    if c == x {
                        continue;
                    }
                    let mut trial = current.clone();
                    trial[i] = c;
                    if let Outcome::Crash(m) = target(&trial, &mut Coverage::new()) {
                        current = trial;
                        message = m;
                        moved = true;
                        break;
                    }
                }
                if !moved {
                    break;
                }
                progress = true;
            }
        }
    }
    Some((current, message))
}

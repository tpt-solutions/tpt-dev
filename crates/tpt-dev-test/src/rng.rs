//! A small, deterministic random number generator.

/// The seed of a test run. The same seed always gives the same cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Seed(pub u64);

/// SplitMix64. Fast, passes common statistical tests, and needs no state beyond one `u64`.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Creates a generator from a seed.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::{Rng, Seed};
    ///
    /// let mut a = Rng::new(Seed(1));
    /// let mut b = Rng::new(Seed(1));
    /// assert_eq!(a.next_u64(), b.next_u64());
    /// ```
    pub const fn new(seed: Seed) -> Self {
        Self { state: seed.0 }
    }

    /// The next 64 random bits.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::{Rng, Seed};
    ///
    /// let mut rng = Rng::new(Seed(1));
    /// assert_ne!(rng.next_u64(), rng.next_u64());
    /// ```
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform value in `lo..=hi`. Uses rejection sampling, so the result is unbiased.
    ///
    /// Returns `lo` when `lo >= hi`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::{Rng, Seed};
    ///
    /// let mut rng = Rng::new(Seed(2));
    /// let v = rng.range_i64(-3, 3);
    /// assert!((-3..=3).contains(&v));
    /// // A single-value range always returns that value.
    /// assert_eq!(rng.range_i64(4, 4), 4);
    /// ```
    pub fn range_i64(&mut self, lo: i64, hi: i64) -> i64 {
        if lo >= hi {
            return lo;
        }
        let span = (i128::from(hi) - i128::from(lo) + 1) as u128;
        let limit = (u64::MAX as u128 + 1) - ((u64::MAX as u128 + 1) % span);
        loop {
            let r = u128::from(self.next_u64());
            if r < limit {
                return (i128::from(lo) + (r % span) as i128) as i64;
            }
        }
    }

    /// A fair coin.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::{Rng, Seed};
    ///
    /// let mut rng = Rng::new(Seed(9));
    /// let flips: Vec<bool> = (0..64).map(|_| rng.bool()).collect();
    /// assert!(flips.contains(&true));
    /// assert!(flips.contains(&false));
    /// ```
    pub fn bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

/// The seed for case `index` of a run with seed `seed`.
///
/// Each case has its own seed, so one failing case can be replayed without the others.
pub(crate) fn case_seed(seed: Seed, index: u32) -> Seed {
    let mut rng = Rng::new(Seed(
        seed.0 ^ (u64::from(index).wrapping_mul(0xD6E8_FEB8_6659_FD93)),
    ));
    Seed(rng.next_u64())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(Seed(42));
        let mut b = Rng::new(Seed(42));
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn ranges_are_inclusive_and_bounded() {
        let mut rng = Rng::new(Seed(7));
        let mut seen = [false; 5];
        for _ in 0..1000 {
            let v = rng.range_i64(-2, 2);
            assert!((-2..=2).contains(&v));
            seen[(v + 2) as usize] = true;
        }
        assert!(seen.iter().all(|&s| s));
        let _ = rng.range_i64(i64::MIN, i64::MAX);
        assert_eq!(rng.range_i64(9, 9), 9);
    }

    #[test]
    fn case_seeds_differ() {
        assert_ne!(case_seed(Seed(1), 0), case_seed(Seed(1), 1));
        assert_eq!(case_seed(Seed(1), 5), case_seed(Seed(1), 5));
    }
}

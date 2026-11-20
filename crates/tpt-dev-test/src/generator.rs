//! Generators: how random test inputs are made, and how failing inputs shrink.

use alloc::vec::Vec;

use crate::rng::Rng;

/// Produces random values of one type, and simpler versions of a failing value.
pub trait Generator {
    /// The type of value produced.
    type Item: Clone;

    /// A random value. `size` is a bound that grows over a run; larger sizes make larger values.
    fn generate(&self, rng: &mut Rng, size: usize) -> Self::Item;

    /// Simpler values to try when `item` fails. Earlier candidates are simpler.
    fn shrink(&self, item: &Self::Item) -> Vec<Self::Item> {
        let _ = item;
        Vec::new()
    }
}

/// Integers in an inclusive range. Shrinks toward zero, or toward the nearest bound if zero is outside the range.
///
/// ```
/// use tpt_dev_test::{Generator, IntRange, Rng, Seed};
///
/// let g = IntRange::new(0, 10);
/// let v = g.generate(&mut Rng::new(Seed(1)), 50);
/// assert!((0..=10).contains(&v));
/// assert!(g.shrink(&9).contains(&0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntRange {
    lo: i64,
    hi: i64,
}

impl IntRange {
    /// The range `lo..=hi`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::{Generator, IntRange, Rng, Seed};
    ///
    /// let range = IntRange::new(1, 6);
    /// let roll = range.generate(&mut Rng::new(Seed(3)), 10);
    /// assert!((1..=6).contains(&roll));
    /// ```
    pub const fn new(lo: i64, hi: i64) -> Self {
        Self { lo, hi }
    }

    fn target(&self) -> i64 {
        0i64.clamp(self.lo, self.hi)
    }
}

impl Generator for IntRange {
    type Item = i64;

    fn generate(&self, rng: &mut Rng, _size: usize) -> i64 {
        rng.range_i64(self.lo, self.hi)
    }

    fn shrink(&self, item: &i64) -> Vec<i64> {
        let target = self.target();
        let x = *item;
        let mut out = Vec::new();
        if x == target {
            return out;
        }
        // Jump to the target, then halve the distance, then step by one.
        out.push(target);
        // Use i128 so the arithmetic cannot overflow at the ends of the i64 range.
        let (t, v) = (i128::from(target), i128::from(x));
        let half = t + (v - t) / 2;
        let step = if v > t { v - 1 } else { v + 1 };
        for c in [t, half, step] {
            if c != v && (i128::from(self.lo)..=i128::from(self.hi)).contains(&c) {
                let c = c as i64;
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        out
    }
}

/// Booleans. `true` shrinks to `false`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bools;

impl Generator for Bools {
    type Item = bool;

    fn generate(&self, rng: &mut Rng, _size: usize) -> bool {
        rng.bool()
    }

    fn shrink(&self, item: &bool) -> Vec<bool> {
        if *item {
            alloc::vec![false]
        } else {
            Vec::new()
        }
    }
}

/// Vectors of up to `max_len` elements (and never more than the run's size).
///
/// Shrinks by emptying the vector, halving it, and removing single elements,
/// then by shrinking each element.
#[derive(Clone, Debug)]
pub struct VecOf<G> {
    elem: G,
    max_len: usize,
}

impl<G> VecOf<G> {
    /// Vectors of `elem` values, up to `max_len` long.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_test::{Bools, Generator, Rng, Seed, VecOf};
    ///
    /// let vectors = VecOf::new(Bools, 4);
    /// let v = vectors.generate(&mut Rng::new(Seed(5)), 10);
    /// assert!(v.len() <= 4);
    /// ```
    pub const fn new(elem: G, max_len: usize) -> Self {
        Self { elem, max_len }
    }
}

impl<G: Generator> Generator for VecOf<G> {
    type Item = Vec<G::Item>;

    fn generate(&self, rng: &mut Rng, size: usize) -> Self::Item {
        let cap = self.max_len.min(size);
        let len = rng.range_i64(0, cap as i64) as usize;
        (0..len).map(|_| self.elem.generate(rng, size)).collect()
    }

    fn shrink(&self, item: &Self::Item) -> Vec<Self::Item> {
        let mut out = Vec::new();
        if item.is_empty() {
            return out;
        }
        out.push(Vec::new());
        let half = item.len() / 2;
        if half > 0 {
            out.push(item[..half].to_vec());
            out.push(item[half..].to_vec());
        }
        for i in 0..item.len().min(16) {
            let mut v = item.clone();
            v.remove(i);
            out.push(v);
        }
        for (i, e) in item.iter().enumerate().take(16) {
            for s in self.elem.shrink(e).into_iter().take(2) {
                let mut v = item.clone();
                v[i] = s;
                out.push(v);
            }
        }
        out
    }
}

/// A pair of values from two generators. Shrinks each component in turn.
#[derive(Clone, Debug)]
pub struct Pair<A, B>(pub A, pub B);

impl<A: Generator, B: Generator> Generator for Pair<A, B> {
    type Item = (A::Item, B::Item);

    fn generate(&self, rng: &mut Rng, size: usize) -> Self::Item {
        (self.0.generate(rng, size), self.1.generate(rng, size))
    }

    fn shrink(&self, item: &Self::Item) -> Vec<Self::Item> {
        let mut out = Vec::new();
        for a in self.0.shrink(&item.0) {
            out.push((a, item.1.clone()));
        }
        for b in self.1.shrink(&item.1) {
            out.push((item.0.clone(), b));
        }
        out
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::rng::Seed;

    #[test]
    fn int_shrinks_toward_zero_and_stays_in_range() {
        let g = IntRange::new(-5, 100);
        for c in g.shrink(&80) {
            assert!((-5..=100).contains(&c));
            assert!(c.abs() < 80);
        }
        assert!(g.shrink(&0).is_empty());
    }

    #[test]
    fn int_shrinks_toward_bound_when_zero_is_outside() {
        let g = IntRange::new(10, 20);
        assert!(g.shrink(&18).contains(&10));
    }

    #[test]
    fn vec_shrinks_to_empty_first() {
        let g = VecOf::new(IntRange::new(0, 9), 8);
        let v = alloc::vec![1, 2, 3];
        assert_eq!(g.shrink(&v)[0], Vec::<i64>::new());
        let generated = g.generate(&mut Rng::new(Seed(3)), 4);
        assert!(generated.len() <= 4);
    }

    #[test]
    fn pair_shrinks_each_side() {
        let g = Pair(IntRange::new(0, 9), Bools);
        let shrunk = g.shrink(&(4, true));
        assert!(shrunk.contains(&(4, false)));
        assert!(shrunk.iter().any(|(a, _)| *a != 4));
    }
}

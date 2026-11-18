//! Exact rational arithmetic on checked `i128` numerator and denominator.
//!
//! Every operation returns `None` on overflow instead of wrapping. The
//! solver turns that `None` into `Unknown`, so it never returns a wrong answer.

use core::cmp::Ordering;

/// An exact rational number `num / den`, always in lowest terms with `den > 0`.
///
/// ```
/// use tpt_dev_solver::Rational;
///
/// let a = Rational::new(1, 3).unwrap();
/// let b = Rational::new(1, 6).unwrap();
/// assert_eq!(a.checked_add(b), Some(Rational::new(1, 2).unwrap()));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rational {
    num: i128,
    den: i128,
}

/// Greatest common divisor of the absolute values. The callers always pass a
/// nonzero positive operand, so the result fits in `i128`.
fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a as i128
}

impl Rational {
    /// Zero.
    pub const ZERO: Self = Self { num: 0, den: 1 };
    /// One.
    pub const ONE: Self = Self { num: 1, den: 1 };

    /// Builds `num / den` in lowest terms. Returns `None` if `den` is zero or
    /// the reduced value does not fit.
    pub fn new(num: i128, den: i128) -> Option<Self> {
        if den == 0 {
            return None;
        }
        let (num, den) = if den < 0 {
            (num.checked_neg()?, den.checked_neg()?)
        } else {
            (num, den)
        };
        let g = gcd(num, den);
        if g == 0 {
            return Some(Self::ZERO);
        }
        Some(Self {
            num: num / g,
            den: den / g,
        })
    }

    /// The integer `n`.
    pub const fn from_int(n: i128) -> Self {
        Self { num: n, den: 1 }
    }

    /// The numerator, sign included.
    pub const fn num(&self) -> i128 {
        self.num
    }

    /// The denominator, always positive.
    pub const fn den(&self) -> i128 {
        self.den
    }

    /// Returns `true` for zero.
    pub const fn is_zero(&self) -> bool {
        self.num == 0
    }

    /// Returns `true` if the value is an integer.
    pub const fn is_integer(&self) -> bool {
        self.den == 1
    }

    /// `-self`, or `None` on overflow.
    pub fn checked_neg(self) -> Option<Self> {
        Some(Self {
            num: self.num.checked_neg()?,
            den: self.den,
        })
    }

    /// `self + other`, or `None` on overflow.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let g = gcd(self.den, other.den).max(1);
        let scale_a = other.den / g;
        let scale_b = self.den / g;
        let num = self
            .num
            .checked_mul(scale_a)?
            .checked_add(other.num.checked_mul(scale_b)?)?;
        let den = self.den.checked_mul(scale_a)?;
        Self::new(num, den)
    }

    /// `self - other`, or `None` on overflow.
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.checked_add(other.checked_neg()?)
    }

    /// `self * other`, or `None` on overflow.
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        let g1 = gcd(self.num, other.den).max(1);
        let g2 = gcd(other.num, self.den).max(1);
        let num = (self.num / g1).checked_mul(other.num / g2)?;
        let den = (self.den / g2).checked_mul(other.den / g1)?;
        Self::new(num, den)
    }

    /// `self / other`, or `None` if `other` is zero or on overflow.
    pub fn checked_div(self, other: Self) -> Option<Self> {
        if other.is_zero() {
            return None;
        }
        let inv = Self::new(other.den, other.num)?;
        self.checked_mul(inv)
    }

    /// The largest integer `<= self`, or `None` if it does not fit.
    pub fn floor(self) -> Option<i128> {
        Some(self.num.div_euclid(self.den))
    }

    /// Returns `true` when the value is below zero.
    pub const fn is_negative(&self) -> bool {
        self.num < 0
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rational {
    /// Compares without multiplying, so it never overflows.
    fn cmp(&self, other: &Self) -> Ordering {
        cmp_fraction(self.num, self.den, other.num, other.den)
    }
}

/// Compares `an/ad` with `bn/bd` for positive denominators, using the
/// continued-fraction expansion, which needs no multiplication.
fn cmp_fraction(an: i128, ad: i128, bn: i128, bd: i128) -> Ordering {
    let qa = an.div_euclid(ad);
    let qb = bn.div_euclid(bd);
    if qa != qb {
        return qa.cmp(&qb);
    }
    let ra = an.rem_euclid(ad);
    let rb = bn.rem_euclid(bd);
    match (ra == 0, rb == 0) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        // ra/ad vs rb/bd is the reverse of ad/ra vs bd/rb.
        (false, false) => cmp_fraction(ad, ra, bd, rb).reverse(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn r(n: i128, d: i128) -> Rational {
        match Rational::new(n, d) {
            Some(v) => v,
            None => panic!("bad test rational"),
        }
    }

    #[test]
    fn normalises() {
        assert_eq!(r(2, 4), r(1, 2));
        assert_eq!(r(3, -6), r(-1, 2));
        assert_eq!(r(0, 7), Rational::ZERO);
        assert!(Rational::new(1, 0).is_none());
    }

    #[test]
    fn arithmetic() {
        assert_eq!(r(1, 3).checked_add(r(1, 6)), Some(r(1, 2)));
        assert_eq!(r(1, 2).checked_sub(r(1, 3)), Some(r(1, 6)));
        assert_eq!(r(2, 3).checked_mul(r(3, 4)), Some(r(1, 2)));
        assert_eq!(r(1, 2).checked_div(r(1, 4)), Some(r(2, 1)));
        assert_eq!(r(1, 2).checked_div(Rational::ZERO), None);
    }

    #[test]
    fn ordering_without_overflow() {
        let big = Rational::new(i128::MAX - 1, i128::MAX).unwrap();
        let other = Rational::new(i128::MAX - 2, i128::MAX - 1).unwrap();
        // (MAX-1)/MAX is closer to 1 than (MAX-2)/(MAX-1).
        assert!(big > other);
        assert!(r(-1, 2) < r(1, 3));
        assert!(r(7, 3) > r(2, 1));
        // 1/15 is larger than 1/26: the fractional parts must compare in the right order.
        assert!(r(1, 15) > r(1, 26));
        assert!(r(1, 26) < r(1, 15));
    }

    #[test]
    fn overflow_is_reported() {
        let big = Rational::from_int(i128::MAX);
        assert_eq!(big.checked_add(Rational::ONE), None);
        assert_eq!(big.checked_mul(Rational::from_int(2)), None);
    }

    #[test]
    fn floor_rounds_down() {
        assert_eq!(r(7, 2).floor(), Some(3));
        assert_eq!(r(-7, 2).floor(), Some(-4));
    }
}

//! Arithmetic in the prime field with modulus 2^61 - 1.
//!
//! This is a small field, chosen so the arithmetic fits in `u128`. It is not a
//! field a proof system is secure over. Production circuits need a 254-bit field
//! such as BN254's scalar field. The `.r1cs` export records the modulus, so the
//! format is right, but the values are not usable with BN254 tools.

use core::ops::{Add, Mul, Neg, Sub};

/// The field modulus, 2^61 - 1 (a Mersenne prime).
pub const MODULUS: u64 = (1 << 61) - 1;

/// An element of the field, always reduced below [`MODULUS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct F(pub(crate) u64);

impl F {
    /// Zero.
    pub const ZERO: F = F(0);
    /// One.
    pub const ONE: F = F(1);

    /// The element congruent to `v`, for negative `v` too.
    pub fn from_i64(v: i64) -> F {
        let m = v.unsigned_abs() % MODULUS;
        if v < 0 && m != 0 {
            F(MODULUS - m)
        } else {
            F(m)
        }
    }

    /// The signed value of the element: values above `MODULUS / 2` are negative.
    pub fn to_i64(self) -> i64 {
        if self.0 > MODULUS / 2 {
            -((MODULUS - self.0) as i64)
        } else {
            self.0 as i64
        }
    }

    /// The reduced representative, in `0..MODULUS`.
    pub fn raw(self) -> u64 {
        self.0
    }
}

fn reduce(x: u128) -> u64 {
    let m = u128::from(MODULUS);
    let x = (x & m) + (x >> 61);
    let x = (x & m) + (x >> 61);
    let mut r = x as u64;
    if r >= MODULUS {
        r -= MODULUS;
    }
    r
}

impl Add for F {
    type Output = F;
    fn add(self, rhs: F) -> F {
        F(reduce(u128::from(self.0) + u128::from(rhs.0)))
    }
}

impl Sub for F {
    type Output = F;
    fn sub(self, rhs: F) -> F {
        F(reduce(u128::from(self.0) + u128::from(MODULUS - rhs.0)))
    }
}

impl Mul for F {
    type Output = F;
    fn mul(self, rhs: F) -> F {
        F(reduce(u128::from(self.0) * u128::from(rhs.0)))
    }
}

impl Neg for F {
    type Output = F;
    fn neg(self) -> F {
        F::ZERO - self
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_matches_wide_integer_reference() {
        // A fixed linear congruential sequence gives repeatable inputs.
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state % MODULUS
        };
        let m = u128::from(MODULUS);
        for _ in 0..10_000 {
            let (a, b) = (next(), next());
            let (fa, fb) = (F(a), F(b));
            assert_eq!(
                (fa * fb).raw(),
                ((u128::from(a) * u128::from(b)) % m) as u64
            );
            assert_eq!(
                (fa + fb).raw(),
                ((u128::from(a) + u128::from(b)) % m) as u64
            );
            assert_eq!((fa - fb + fb), fa);
        }
    }

    #[test]
    fn signed_values_round_trip() {
        for v in [-5i64, -1, 0, 1, 42, -1_000_000] {
            assert_eq!(F::from_i64(v).to_i64(), v);
        }
        assert_eq!(F::from_i64(-1) + F::ONE, F::ZERO);
        assert_eq!(-F::ONE, F::from_i64(-1));
    }
}

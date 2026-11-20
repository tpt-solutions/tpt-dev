//! Arithmetic in the BN254 scalar field, the field circom and snarkjs use.
//!
//! The modulus is r = 21888242871839275222246405745257275088548364400416034343698204186575808495617,
//! a 254-bit prime. Elements are kept in Montgomery form, four 64-bit limbs,
//! and multiplied with the CIOS method. Every stored value is reduced below r.
//! The constants were computed with Python's integer arithmetic, and the tests
//! check the operations against values from the same independent computation.

use core::ops::{Add, Mul, Neg, Sub};

/// The BN254 scalar field modulus r, as four little-endian 64-bit limbs.
const M: [u64; 4] = [
    0x43e1_f593_f000_0001,
    0x2833_e848_79b9_7091,
    0xb850_45b6_8181_585d,
    0x3064_4e72_e131_a029,
];

/// R^2 mod r, where R = 2^256. Multiplying by it converts into Montgomery form.
const R2: [u64; 4] = [
    0x1bb8_e645_ae21_6da7,
    0x53fe_3ab1_e35c_59e3,
    0x8c49_833d_53bb_8085,
    0x0216_d0b1_7f4e_44a5,
];

/// -r^-1 mod 2^64, used by the Montgomery reduction.
const N0: u64 = 0xc2e1_f593_efff_ffff;

/// The modulus r, as 32 little-endian bytes. This is the value the `.r1cs` header records.
pub const MODULUS_LE: [u8; 32] = limbs_to_le(M);

const fn limbs_to_le(limbs: [u64; 4]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 4 {
        let bytes = limbs[i].to_le_bytes();
        let mut k = 0;
        while k < 8 {
            out[i * 8 + k] = bytes[k];
            k += 1;
        }
        i += 1;
    }
    out
}

const fn le_to_limbs(bytes: &[u8; 32]) -> [u64; 4] {
    let mut out = [0u64; 4];
    let mut i = 0;
    while i < 4 {
        let mut chunk = [0u8; 8];
        let mut k = 0;
        while k < 8 {
            chunk[k] = bytes[i * 8 + k];
            k += 1;
        }
        out[i] = u64::from_le_bytes(chunk);
        i += 1;
    }
    out
}

/// Whether `a >= b`, comparing from the most significant limb.
const fn geq(a: [u64; 4], b: [u64; 4]) -> bool {
    let mut i = 4;
    while i > 0 {
        i -= 1;
        if a[i] != b[i] {
            return a[i] > b[i];
        }
    }
    true
}

/// `a - b` without reduction, and whether it borrowed.
const fn sub_raw(a: [u64; 4], b: [u64; 4]) -> ([u64; 4], bool) {
    let mut out = [0u64; 4];
    let mut borrow = 0u64;
    let mut i = 0;
    while i < 4 {
        let (d1, b1) = a[i].overflowing_sub(b[i]);
        let (d2, b2) = d1.overflowing_sub(borrow);
        out[i] = d2;
        borrow = (b1 | b2) as u64;
        i += 1;
    }
    (out, borrow != 0)
}

/// `a + b` without reduction, and whether it carried out.
const fn add_raw(a: [u64; 4], b: [u64; 4]) -> ([u64; 4], bool) {
    let mut out = [0u64; 4];
    let mut carry = 0u64;
    let mut i = 0;
    while i < 4 {
        let (s1, c1) = a[i].overflowing_add(b[i]);
        let (s2, c2) = s1.overflowing_add(carry);
        out[i] = s2;
        carry = (c1 | c2) as u64;
        i += 1;
    }
    (out, carry != 0)
}

/// Montgomery product: `a * b * R^-1 mod r`, for inputs below r.
const fn mul_mont(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    let mut t = [0u64; 6];
    let mut i = 0;
    while i < 4 {
        let mut carry: u128 = 0;
        let mut j = 0;
        while j < 4 {
            let s = t[j] as u128 + (a[j] as u128) * (b[i] as u128) + carry;
            t[j] = s as u64;
            carry = s >> 64;
            j += 1;
        }
        let s = t[4] as u128 + carry;
        t[4] = s as u64;
        t[5] = (s >> 64) as u64;

        let m = t[0].wrapping_mul(N0);
        let s = t[0] as u128 + (m as u128) * (M[0] as u128);
        let mut carry = s >> 64;
        let mut j = 1;
        while j < 4 {
            let s = t[j] as u128 + (m as u128) * (M[j] as u128) + carry;
            t[j - 1] = s as u64;
            carry = s >> 64;
            j += 1;
        }
        let s = t[4] as u128 + carry;
        t[3] = s as u64;
        let carry = (s >> 64) as u64;
        t[4] = t[5] + carry;
        t[5] = 0;
        i += 1;
    }
    let r = [t[0], t[1], t[2], t[3]];
    if t[4] != 0 || geq(r, M) {
        sub_raw(r, M).0
    } else {
        r
    }
}

/// An element of the BN254 scalar field, in Montgomery form and always reduced below r.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct F(pub(crate) [u64; 4]);

impl F {
    /// Zero.
    pub const ZERO: F = F([0; 4]);
    /// One, in Montgomery form.
    pub const ONE: F = F(mul_mont([1, 0, 0, 0], R2));

    /// The field element with the given canonical value, as little-endian bytes.
    ///
    /// Returns `None` if the value is not below the modulus.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_zk::{F, MODULUS_LE};
    ///
    /// let mut five = [0u8; 32];
    /// five[0] = 5;
    /// assert_eq!(F::from_bytes_le(&five).map(|f| f.to_i64()), Some(5));
    /// // The modulus itself is not an element of the field.
    /// assert!(F::from_bytes_le(&MODULUS_LE).is_none());
    /// ```
    pub fn from_bytes_le(bytes: &[u8; 32]) -> Option<F> {
        let canonical = le_to_limbs(bytes);
        if geq(canonical, M) {
            None
        } else {
            Some(F(mul_mont(canonical, R2)))
        }
    }

    /// The canonical value, as 32 little-endian bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_zk::{F, MODULUS_LE};
    ///
    /// // -1 is the modulus minus one, so its bytes are the modulus with the low byte lowered.
    /// let bytes = F::from_i64(-1).to_bytes_le();
    /// assert_eq!(bytes[0], MODULUS_LE[0] - 1);
    /// assert_eq!(bytes[1..], MODULUS_LE[1..]);
    /// ```
    pub fn to_bytes_le(self) -> [u8; 32] {
        limbs_to_le(self.canonical())
    }

    fn canonical(self) -> [u64; 4] {
        mul_mont(self.0, [1, 0, 0, 0])
    }

    /// The element congruent to `v`, for negative `v` too.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_zk::F;
    ///
    /// let minus_three = F::from_i64(-3);
    /// let three = F::from_i64(3);
    /// assert_eq!(minus_three + three, F::from_i64(0));
    /// ```
    pub fn from_i64(v: i64) -> F {
        let magnitude = [v.unsigned_abs(), 0, 0, 0];
        let canonical = if v < 0 && magnitude[0] != 0 {
            sub_raw(M, magnitude).0
        } else {
            magnitude
        };
        F(mul_mont(canonical, R2))
    }

    /// The signed value: canonical values above `(r - 1) / 2` are negative.
    ///
    /// A value that does not fit in 64 bits is truncated to its low 64 bits,
    /// so this is exact only for small values.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_zk::F;
    ///
    /// assert_eq!(F::from_i64(-7).to_i64(), -7);
    /// assert_eq!(F::from_i64(7).to_i64(), 7);
    /// ```
    pub fn to_i64(self) -> i64 {
        let c = self.canonical();
        let half = [
            (M[0] >> 1) | (M[1] << 63),
            (M[1] >> 1) | (M[2] << 63),
            (M[2] >> 1) | (M[3] << 63),
            M[3] >> 1,
        ];
        if geq(c, half) && c != half {
            let magnitude = sub_raw(M, c).0;
            -(magnitude[0] as i64)
        } else {
            c[0] as i64
        }
    }
}

impl Add for F {
    type Output = F;
    fn add(self, rhs: F) -> F {
        let (sum, carry) = add_raw(self.0, rhs.0);
        if carry || geq(sum, M) {
            F(sub_raw(sum, M).0)
        } else {
            F(sum)
        }
    }
}

impl Sub for F {
    type Output = F;
    fn sub(self, rhs: F) -> F {
        let (diff, borrow) = sub_raw(self.0, rhs.0);
        if borrow {
            F(add_raw(diff, M).0)
        } else {
            F(diff)
        }
    }
}

impl Mul for F {
    type Output = F;
    fn mul(self, rhs: F) -> F {
        F(mul_mont(self.0, rhs.0))
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

    fn unhex(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
        }
        out
    }

    /// Products, sums and differences computed by an independent big-integer
    /// implementation: each line is a, b, a*b, a+b, a-b, as 32-byte little-endian hex.
    #[test]
    fn operations_match_independent_big_integer_vectors() {
        let text = include_str!("../tests/fixtures/bn254_vectors.txt");
        let mut count = 0;
        for line in text.lines() {
            let parts: std::vec::Vec<[u8; 32]> = line.split_whitespace().map(unhex).collect();
            assert_eq!(parts.len(), 5, "malformed vector line");
            let (a, b, mul, add, sub) = (parts[0], parts[1], parts[2], parts[3], parts[4]);
            let fa = F::from_bytes_le(&a).unwrap();
            let fb = F::from_bytes_le(&b).unwrap();
            assert_eq!((fa * fb).to_bytes_le(), mul);
            assert_eq!((fa + fb).to_bytes_le(), add);
            assert_eq!((fa - fb).to_bytes_le(), sub);
            count += 1;
        }
        assert_eq!(count, 14);
    }

    #[test]
    fn the_modulus_is_refused_and_the_value_below_it_accepted() {
        assert!(F::from_bytes_le(&MODULUS_LE).is_none());
        let mut below = MODULUS_LE;
        below[0] -= 1;
        let f = F::from_bytes_le(&below).unwrap();
        assert_eq!(f.to_bytes_le(), below);
    }

    #[test]
    fn signed_values_round_trip() {
        for v in [-5i64, -1, 0, 1, 42, -1_000_000, i64::MAX, i64::MIN + 1] {
            assert_eq!(F::from_i64(v).to_i64(), v, "v = {v}");
        }
        assert_eq!(F::from_i64(-1) + F::ONE, F::ZERO);
        assert_eq!(-F::ONE, F::from_i64(-1));
        assert_eq!(F::ONE.to_bytes_le()[0], 1);
    }

    #[test]
    fn zero_and_one_behave_as_identities() {
        let x = F::from_i64(123_456_789);
        assert_eq!(x + F::ZERO, x);
        assert_eq!(x * F::ONE, x);
        assert_eq!(x - x, F::ZERO);
    }
}

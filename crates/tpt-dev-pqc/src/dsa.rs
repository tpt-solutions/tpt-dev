//! ML-DSA (FIPS 204): a digital signature scheme from module lattices.
//!
//! Signing uses rejection sampling: it draws a masking vector, checks that the
//! response leaks nothing about the secret, and retries when a check fails. The
//! retries are the scheme's design. To keep their number from showing in the
//! timing, signing makes a fixed budget of attempts and selects the first passing
//! one with masks. See `sign_internal`. The budget is exceeded only with small
//! probability, and then the standard loop continues, which is not constant time.

use alloc::vec::Vec;
use core::marker::PhantomData;

use crate::keccak::Sponge;
use crate::kem::Rng;

/// The modulus q = 2^23 - 2^13 + 1.
const Q: i32 = 8_380_417;
const N: usize = 256;
/// Number of dropped low bits of the public key's t.
const D: u32 = 13;
/// The primitive 512th root of unity mod q.
const ZETA: i64 = 1753;
/// 256^-1 mod q, which scales the inverse NTT.
const N_INV: i64 = 8_347_681;
/// The Montgomery radix is R = 2^32. `R mod q`, used below to build constants.
const R_MOD_Q: i64 = (1i64 << 32) % Q as i64;
/// R^2 mod q, which takes a product of two residues back into the normal domain.
const R2_MOD_Q: u64 = (R_MOD_Q * R_MOD_Q % Q as i64) as u64;
/// -q^-1 mod 2^32, the constant of the Montgomery reduction.
const Q_NEG_INV: u32 = neg_inv_mod_2_32(Q as u32);
/// Attempts that one signing call always makes. A signature is produced in the
/// first attempt that passes, as in the standard loop, so the budget changes only
/// the work done, not the output. With an acceptance chance of about 0.2 to 0.26
/// per attempt (FIPS 204 expected attempts: 4.25, 5.1, 3.85), the chance of using
/// all 64 is below 1e-6 for each level.
const SIGN_ATTEMPTS: u16 = 64;

type Poly = [i32; N];

const ZERO: Poly = [0; N];

/// Parameters of one ML-DSA security level.
pub trait Parameters: Clone + core::fmt::Debug + PartialEq + Eq {
    /// Rows of the matrix A: the length of t and s2.
    const K: usize;
    /// Columns of the matrix A: the length of s1 and y.
    const L: usize;
    /// Bound on the secret coefficients.
    const ETA: i32;
    /// Number of nonzero coefficients in the challenge polynomial.
    const TAU: usize;
    /// Range of the masking vector, gamma1.
    const GAMMA1: i32;
    /// Low-order rounding range, gamma2.
    const GAMMA2: i32;
    /// Maximum number of hint bits.
    const OMEGA: usize;
    /// Length of the challenge hash, lambda / 4 bytes.
    const CTILDE: usize;
    /// The level's name, for messages.
    const NAME: &'static str;
}

/// ML-DSA-44, NIST security category 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MlDsa44;

/// ML-DSA-65, NIST security category 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MlDsa65;

/// ML-DSA-87, NIST security category 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MlDsa87;

impl Parameters for MlDsa44 {
    const K: usize = 4;
    const L: usize = 4;
    const ETA: i32 = 2;
    const TAU: usize = 39;
    const GAMMA1: i32 = 1 << 17;
    const GAMMA2: i32 = (Q - 1) / 88;
    const OMEGA: usize = 80;
    const CTILDE: usize = 32;
    const NAME: &'static str = "ML-DSA-44";
}

impl Parameters for MlDsa65 {
    const K: usize = 6;
    const L: usize = 5;
    const ETA: i32 = 4;
    const TAU: usize = 49;
    const GAMMA1: i32 = 1 << 19;
    const GAMMA2: i32 = (Q - 1) / 32;
    const OMEGA: usize = 55;
    const CTILDE: usize = 48;
    const NAME: &'static str = "ML-DSA-65";
}

impl Parameters for MlDsa87 {
    const K: usize = 8;
    const L: usize = 7;
    const ETA: i32 = 2;
    const TAU: usize = 60;
    const GAMMA1: i32 = 1 << 19;
    const GAMMA2: i32 = (Q - 1) / 32;
    const OMEGA: usize = 75;
    const CTILDE: usize = 64;
    const NAME: &'static str = "ML-DSA-87";
}

/// Why a key, signature or context could not be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DsaError {
    /// The byte string has the wrong length for this parameter set.
    InvalidLength,
    /// The context string is longer than 255 bytes.
    ContextTooLong,
}

impl core::fmt::Display for DsaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            DsaError::InvalidLength => "wrong length for this ML-DSA parameter set",
            DsaError::ContextTooLong => "context string is longer than 255 bytes",
        })
    }
}

/// Overwrites `bytes` with zeros, on a best-effort basis.
fn wipe(bytes: &mut [u8]) {
    for b in bytes.iter_mut() {
        *b = 0;
    }
    core::hint::black_box(bytes);
}

const fn pow_mod(mut base: i64, mut exp: u32) -> i64 {
    let mut r = 1;
    while exp > 0 {
        if exp & 1 == 1 {
            r = r * base % Q as i64;
        }
        base = base * base % Q as i64;
        exp >>= 1;
    }
    r
}

const fn bit_reverse8(x: u32) -> u32 {
    let mut r = 0;
    let mut i = 0;
    while i < 8 {
        r |= ((x >> i) & 1) << (7 - i);
        i += 1;
    }
    r
}

/// zeta^BitRev8(m), the twiddle factors of the NTT.
const ZETAS: [i32; N] = {
    let mut t = [0i32; N];
    let mut m = 0;
    while m < N {
        t[m] = pow_mod(ZETA, bit_reverse8(m as u32)) as i32;
        m += 1;
    }
    t
};

/// -q^-1 mod 2^32 by Newton's iteration. An odd q is its own inverse mod 8, and
/// each step doubles the number of correct low bits.
const fn neg_inv_mod_2_32(q: u32) -> u32 {
    let mut inv = q;
    let mut i = 0;
    while i < 5 {
        inv = inv.wrapping_mul(2u32.wrapping_sub(q.wrapping_mul(inv)));
        i += 1;
    }
    inv.wrapping_neg()
}

/// The twiddle factors in Montgomery form, zeta^BitRev8(m) * R mod q. A product
/// of one of these with a residue, reduced by `redc`, gives the plain product.
const ZETAS_MONT: [i32; N] = {
    let mut t = [0i32; N];
    let mut m = 0;
    while m < N {
        t[m] = (ZETAS[m] as i64 * R_MOD_Q % Q as i64) as i32;
        m += 1;
    }
    t
};

/// 256^-1 * R mod q, the scaling constant of the inverse NTT in Montgomery form.
const N_INV_MONT: u64 = (N_INV * R_MOD_Q % Q as i64) as u64;

/// Reduces `x` to `0..q`. The sign correction is a mask, not a branch.
#[inline(always)]
fn modq(x: i32) -> i32 {
    let r = x % Q;
    r + (Q & (r >> 31))
}

/// Montgomery reduction: returns `t * 2^-32 mod q` in `0..q`, for `t < q * 2^32`.
/// It uses only 32-bit multiplies and 64-bit adds, so on Cortex-M it needs no
/// 64-bit division library call (`__aeabi_ldivmod`).
#[inline(always)]
fn redc(t: u64) -> i32 {
    let m = (t as u32).wrapping_mul(Q_NEG_INV);
    // t + m*q is a multiple of 2^32, and its quotient is below 2q.
    let u = ((t + u64::from(m) * Q as u64) >> 32) as i32;
    let r = u - Q;
    r + (Q & (r >> 31))
}

/// Maps a residue in `0..q` to the centred range `(-q/2, q/2]`, without branching.
#[inline(always)]
fn center(x: i32) -> i32 {
    // x > q/2 exactly when q/2 - x is negative.
    x - (Q & ((Q / 2 - x) >> 31))
}

/// 1 when `|v| < bound`, else 0. Both values are small, so nothing overflows.
#[inline(always)]
fn abs_below(v: i32, bound: i32) -> u32 {
    let s = v >> 31;
    let a = (v ^ s) - s;
    (((a - bound) >> 31) & 1) as u32
}

/// 1 when every coefficient of `polys`, centred, has absolute value below
/// `bound`, else 0. Every coefficient is visited, so the work is fixed.
fn centred_within(polys: &[Poly], bound: i32) -> u32 {
    let mut ok = 1;
    for p in polys {
        for &c in p {
            ok &= abs_below(center(c), bound);
        }
    }
    ok
}

/// 1 when every low-order part `r0` of `polys` has absolute value below
/// `bound`, else 0. Every coefficient is visited, so the work is fixed.
fn low_within(polys: &[Poly], gamma2: i32, bound: i32) -> u32 {
    let mut ok = 1;
    for p in polys {
        for &c in p {
            ok &= abs_below(low_bits(c, gamma2), bound);
        }
    }
    ok
}

fn ntt(w: &mut Poly) {
    let mut m = 0;
    let mut len = 128;
    while len >= 1 {
        let mut start = 0;
        while start < N {
            m += 1;
            let z = ZETAS_MONT[m] as u64;
            for j in start..start + len {
                let t = redc(z * u64::from(w[j + len] as u32));
                w[j + len] = modq(w[j] - t);
                w[j] = modq(w[j] + t);
            }
            start += 2 * len;
        }
        len /= 2;
    }
}

fn inverse_ntt(w: &mut Poly) {
    let mut m = N;
    let mut len = 1;
    while len < N {
        let mut start = 0;
        while start < N {
            m -= 1;
            let z = modq(-ZETAS_MONT[m]) as u64;
            for j in start..start + len {
                let t = w[j];
                w[j] = modq(t + w[j + len]);
                w[j + len] = modq(t - w[j + len]);
                w[j + len] = redc(z * u64::from(w[j + len] as u32));
            }
            start += 2 * len;
        }
        len *= 2;
    }
    for c in w.iter_mut() {
        *c = redc(N_INV_MONT * u64::from(*c as u32));
    }
}

/// Pointwise product of two polynomials in the NTT domain.
fn pointwise(a: &Poly, b: &Poly) -> Poly {
    let mut out = ZERO;
    for i in 0..N {
        // The first reduction leaves a*b/R, and the second, with R^2, gives a*b.
        let ab = redc(u64::from(a[i] as u32) * u64::from(b[i] as u32));
        out[i] = redc(u64::from(ab as u32) * R2_MOD_Q);
    }
    out
}

fn add(a: &Poly, b: &Poly) -> Poly {
    let mut out = ZERO;
    for i in 0..N {
        out[i] = modq(a[i] + b[i]);
    }
    out
}

fn sub(a: &Poly, b: &Poly) -> Poly {
    let mut out = ZERO;
    for i in 0..N {
        out[i] = modq(a[i] - b[i]);
    }
    out
}

/// `Power2Round`: splits r into `(r1, r0)` with `r = r1 * 2^d + r0`, and `r0` centred.
fn power2round(r: i32) -> (i32, i32) {
    let rp = modq(r);
    let mut r0 = rp & ((1 << D) - 1);
    if r0 > (1 << (D - 1)) {
        r0 -= 1 << D;
    }
    ((rp - r0) >> D, r0)
}

/// `Decompose`: splits r into `(r1, r0)` with `r = r1 * 2 * gamma2 + r0`.
///
/// Branch-free. `gamma2` is a constant at every call site in this module, and
/// these functions are inlined so the division by `2 * gamma2` is by a constant.
#[inline(always)]
fn decompose(r: i32, gamma2: i32) -> (i32, i32) {
    let rp = modq(r);
    let alpha = 2 * gamma2;
    let mut r0 = rp % alpha;
    // if r0 > gamma2 { r0 -= alpha }; gamma2 - r0 is negative exactly then.
    r0 -= alpha & ((gamma2 - r0) >> 31);
    // if rp - r0 == q - 1 { (0, r0 - 1) }
    let d = (rp - r0) ^ (Q - 1);
    let is_edge = (((d as u32) | (d as u32).wrapping_neg()) >> 31) ^ 1;
    let edge = (is_edge as i32).wrapping_neg();
    (((rp - r0) / alpha) & !edge, r0 + edge)
}

#[inline(always)]
fn high_bits(r: i32, gamma2: i32) -> i32 {
    decompose(r, gamma2).0
}

#[inline(always)]
fn low_bits(r: i32, gamma2: i32) -> i32 {
    decompose(r, gamma2).1
}

/// `MakeHint` as 0 or 1: whether adding z to r changes its high bits. Branch-free.
#[inline(always)]
fn make_hint(z: i32, r: i32, gamma2: i32) -> i32 {
    let d = (high_bits(r, gamma2) ^ high_bits(modq(r + z), gamma2)) as u32;
    ((d | d.wrapping_neg()) >> 31) as i32
}

/// `UseHint`: recovers the high bits of r + z from r and the hint.
fn use_hint(hint: bool, r: i32, gamma2: i32) -> i32 {
    let m = (Q - 1) / (2 * gamma2);
    let (r1, r0) = decompose(r, gamma2);
    if !hint {
        r1
    } else if r0 > 0 {
        (r1 + 1) % m
    } else {
        (r1 - 1).rem_euclid(m)
    }
}

/// Packs `values`, each `bits` wide, least significant bit first.
fn pack(values: &[u32], bits: u32, out: &mut Vec<u8>) {
    let mut acc = 0u64;
    let mut n = 0u32;
    for &v in values {
        acc |= u64::from(v) << n;
        n += bits;
        while n >= 8 {
            out.push(acc as u8);
            acc >>= 8;
            n -= 8;
        }
    }
    if n > 0 {
        out.push(acc as u8);
    }
}

/// Unpacks `count` values, each `bits` wide.
fn unpack(bytes: &[u8], bits: u32, count: usize) -> Vec<u32> {
    let mask = (1u64 << bits) - 1;
    let mut out = Vec::with_capacity(count);
    let mut acc = 0u64;
    let mut n = 0u32;
    let mut idx = 0;
    for _ in 0..count {
        while n < bits {
            acc |= u64::from(bytes[idx]) << n;
            idx += 1;
            n += 8;
        }
        out.push((acc & mask) as u32);
        acc >>= bits;
        n -= bits;
    }
    out
}

/// Bits needed to hold `x`.
fn bit_len(x: u32) -> u32 {
    32 - x.leading_zeros()
}

/// Expands the public matrix A, in the NTT domain, from `rho`.
fn expand_a<P: Parameters>(rho: &[u8; 32]) -> Vec<Vec<Poly>> {
    (0..P::K)
        .map(|i| {
            (0..P::L)
                .map(|j| rej_ntt_poly(rho, j as u8, i as u8))
                .collect()
        })
        .collect()
}

/// RejNTTPoly: uniform coefficients in `0..q`, from SHAKE128 of `rho || x || y`.
fn rej_ntt_poly(rho: &[u8; 32], x: u8, y: u8) -> Poly {
    let mut xof = Sponge::new(168);
    xof.absorb(rho);
    xof.absorb(&[x, y]);
    xof.finish(0x1f);
    let mut out = ZERO;
    let mut n = 0;
    let mut buf = [0u8; 3];
    while n < N {
        xof.squeeze(&mut buf);
        let v = u32::from(buf[0]) | (u32::from(buf[1]) << 8) | ((u32::from(buf[2]) & 0x7f) << 16);
        if v < Q as u32 {
            out[n] = v as i32;
            n += 1;
        }
    }
    out
}

/// RejBoundedPoly: coefficients in `[-eta, eta]`, from SHAKE256 of `seed`.
fn rej_bounded_poly(seed: &[u8], eta: i32) -> Poly {
    let mut xof = Sponge::new(136);
    xof.absorb(seed);
    xof.finish(0x1f);
    let mut out = ZERO;
    let mut n = 0;
    let mut buf = [0u8; 1];
    while n < N {
        xof.squeeze(&mut buf);
        for nibble in [i32::from(buf[0] & 0x0f), i32::from(buf[0] >> 4)] {
            if n == N {
                break;
            }
            let value = match eta {
                2 if nibble < 15 => Some(2 - nibble % 5),
                4 if nibble < 9 => Some(4 - nibble),
                _ => None,
            };
            if let Some(v) = value {
                out[n] = modq(v);
                n += 1;
            }
        }
    }
    out
}

/// ExpandS: the secret vectors s1 (length L) and s2 (length K), from `rho_prime`.
fn expand_s<P: Parameters>(rho_prime: &[u8; 64]) -> (Vec<Poly>, Vec<Poly>) {
    let seed = |r: usize| {
        let mut s = Vec::with_capacity(66);
        s.extend_from_slice(rho_prime);
        s.extend_from_slice(&(r as u16).to_le_bytes());
        s
    };
    let s1 = (0..P::L)
        .map(|r| rej_bounded_poly(&seed(r), P::ETA))
        .collect();
    let s2 = (0..P::K)
        .map(|r| rej_bounded_poly(&seed(P::L + r), P::ETA))
        .collect();
    (s1, s2)
}

/// ExpandMask: the masking vector y for one attempt, counter `kappa`.
fn expand_mask<P: Parameters>(rho: &[u8; 64], kappa: u16) -> Vec<Poly> {
    let bits = bit_len((P::GAMMA1 - 1) as u32) + 1;
    (0..P::L)
        .map(|r| {
            let mut seed = Vec::with_capacity(66);
            seed.extend_from_slice(rho);
            seed.extend_from_slice(&(kappa + r as u16).to_le_bytes());
            let mut bytes = alloc::vec![0u8; 32 * bits as usize];
            let mut xof = Sponge::new(136);
            xof.absorb(&seed);
            xof.finish(0x1f);
            xof.squeeze(&mut bytes);
            let values = unpack(&bytes, bits, N);
            let mut y = ZERO;
            for (c, v) in y.iter_mut().zip(values) {
                *c = modq(P::GAMMA1 - v as i32);
            }
            y
        })
        .collect()
}

/// SampleInBall: a challenge polynomial with `tau` coefficients of plus or minus one.
fn sample_in_ball(seed: &[u8], tau: usize) -> Poly {
    let mut xof = Sponge::new(136);
    xof.absorb(seed);
    xof.finish(0x1f);
    let mut signs_bytes = [0u8; 8];
    xof.squeeze(&mut signs_bytes);
    let mut signs = u64::from_le_bytes(signs_bytes);
    let mut c = ZERO;
    let mut byte = [0u8; 1];
    for i in (N - tau)..N {
        xof.squeeze(&mut byte);
        let mut j = usize::from(byte[0]);
        while j > i {
            xof.squeeze(&mut byte);
            j = usize::from(byte[0]);
        }
        c[i] = c[j];
        c[j] = if signs & 1 == 1 { Q - 1 } else { 1 };
        signs >>= 1;
    }
    c
}

/// Hashes the concatenation of `parts` with SHAKE256, into `out`.
fn shake256(parts: &[&[u8]], out: &mut [u8]) {
    let mut xof = Sponge::new(136);
    for p in parts {
        xof.absorb(p);
    }
    xof.finish(0x1f);
    xof.squeeze(out);
}

/// Packs the centred coefficients `c` of `p` as `base - c`, in `bits` bits each.
fn pack_centered(p: &Poly, base: i32, bits: u32, out: &mut Vec<u8>) {
    let values: Vec<u32> = p.iter().map(|&c| (base - center(c)) as u32).collect();
    pack(&values, bits, out);
}

/// Unpacks `bits`-wide values as `base - v`, reduced mod q.
fn unpack_centered(bytes: &[u8], base: i32, bits: u32) -> Poly {
    let values = unpack(bytes, bits, N);
    let mut p = ZERO;
    for (c, v) in p.iter_mut().zip(values) {
        *c = modq(base - v as i32);
    }
    p
}

/// The number of bytes of one packed polynomial of `bits`-wide values.
const fn poly_bytes(bits: u32) -> usize {
    32 * bits as usize
}

/// Bits per coefficient of w1, the packed high bits of the commitment.
fn w1_bits<P: Parameters>() -> u32 {
    bit_len(((Q - 1) / (2 * P::GAMMA2) - 1) as u32)
}

/// Bits per coefficient of the masking vector z.
fn z_bits<P: Parameters>() -> u32 {
    bit_len((2 * P::GAMMA1 - 1) as u32)
}

/// Bits per coefficient of the secret s1 and s2.
fn eta_bits<P: Parameters>() -> u32 {
    bit_len((2 * P::ETA) as u32)
}

/// Encoded length of the public key.
fn pk_len<P: Parameters>() -> usize {
    32 + P::K * poly_bytes(10)
}

/// Encoded length of the secret key.
fn sk_len<P: Parameters>() -> usize {
    32 + 32 + 64 + (P::L + P::K) * poly_bytes(eta_bits::<P>()) + P::K * poly_bytes(D)
}

/// Encoded length of a signature.
fn sig_len<P: Parameters>() -> usize {
    P::CTILDE + P::L * poly_bytes(z_bits::<P>()) + P::OMEGA + P::K
}

/// Encodes the high bits of the commitment vector w1.
fn w1_encode<P: Parameters>(w1: &[Poly]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in w1 {
        let values: Vec<u32> = p.iter().map(|&c| c as u32).collect();
        pack(&values, w1_bits::<P>(), &mut out);
    }
    out
}

/// HintBitPack: the hint as positions of ones, then the count per polynomial.
///
/// Branch-free. Every coefficient writes its index to the next free slot, and
/// the slot advances only for a one. A malformed (too heavy) hint gives output
/// the caller must discard, and the caller does.
fn hint_pack<P: Parameters>(h: &[Poly]) -> Vec<u8> {
    let mut out = alloc::vec![0u8; P::OMEGA + P::K];
    let mut slots = alloc::vec![0u8; P::K * N];
    let mut index = 0usize;
    for (i, row) in h.iter().enumerate() {
        for (j, &bit) in row.iter().enumerate() {
            slots[index] = j as u8;
            index += bit as usize;
        }
        out[P::OMEGA + i] = index as u8;
    }
    // Keep the first `index` positions and zero the rest, as the packing requires.
    for (k, o) in out.iter_mut().take(P::OMEGA).enumerate() {
        let keep = (((k as i32) - (index as i32)) >> 31) as u8;
        *o = slots[k] & keep;
    }
    out
}

/// HintBitUnpack. Returns `None` for a malformed hint.
fn hint_unpack<P: Parameters>(y: &[u8]) -> Option<Vec<Poly>> {
    let mut h = alloc::vec![ZERO; P::K];
    let mut k = 0usize;
    for i in 0..P::K {
        let limit = usize::from(y[P::OMEGA + i]);
        if limit < k || limit > P::OMEGA {
            return None;
        }
        for j in k..limit {
            if j > k && y[j] <= y[j - 1] {
                return None;
            }
            h[i][usize::from(y[j])] = 1;
        }
        k = limit;
    }
    for &extra in &y[k..P::OMEGA] {
        if extra != 0 {
            return None;
        }
    }
    Some(h)
}

/// A public (verification) key for parameter set `P`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyingKey<P: Parameters> {
    bytes: Vec<u8>,
    _params: PhantomData<P>,
}

impl<P: Parameters> VerifyingKey<P> {
    /// The encoded length for this parameter set.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_pqc::{MlDsa65, VerifyingKey};
    ///
    /// // ML-DSA-65 verifying keys are 1952 bytes.
    /// assert_eq!(VerifyingKey::<MlDsa65>::encoded_len(), 1952);
    /// ```
    pub fn encoded_len() -> usize {
        pk_len::<P>()
    }

    /// Parses a public key, checking its length.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_pqc::{MlDsa44, VerifyingKey, dsa_keygen_from_seed};
    ///
    /// let (vk, _sk) = dsa_keygen_from_seed::<MlDsa44>(&[1; 32]);
    /// let parsed = VerifyingKey::<MlDsa44>::from_bytes(vk.as_bytes()).unwrap();
    /// assert_eq!(parsed.as_bytes(), vk.as_bytes());
    /// // A key of the wrong length is refused.
    /// assert!(VerifyingKey::<MlDsa44>::from_bytes(&[0u8; 10]).is_err());
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DsaError> {
        if bytes.len() != pk_len::<P>() {
            return Err(DsaError::InvalidLength);
        }
        Ok(Self {
            bytes: bytes.to_vec(),
            _params: PhantomData,
        })
    }

    /// The encoded key.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_pqc::{MlDsa44, VerifyingKey, dsa_keygen_from_seed};
    ///
    /// let (vk, _sk) = dsa_keygen_from_seed::<MlDsa44>(&[1; 32]);
    /// assert_eq!(vk.as_bytes().len(), VerifyingKey::<MlDsa44>::encoded_len());
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A secret (signing) key for parameter set `P`. It is wiped when dropped, on a
/// best-effort basis. It has no `Debug` or `Clone`.
pub struct SigningKey<P: Parameters> {
    bytes: Vec<u8>,
    _params: PhantomData<P>,
}

impl<P: Parameters> SigningKey<P> {
    /// The encoded length for this parameter set.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_pqc::{MlDsa44, SigningKey};
    ///
    /// // ML-DSA-44 signing keys are 2560 bytes.
    /// assert_eq!(SigningKey::<MlDsa44>::encoded_len(), 2560);
    /// ```
    pub fn encoded_len() -> usize {
        sk_len::<P>()
    }

    /// Parses a secret key, checking its length.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_pqc::{MlDsa44, SigningKey, dsa_keygen_from_seed};
    ///
    /// let (_vk, sk) = dsa_keygen_from_seed::<MlDsa44>(&[1; 32]);
    /// let parsed = SigningKey::<MlDsa44>::from_bytes(sk.as_bytes()).unwrap();
    /// assert_eq!(parsed.as_bytes(), sk.as_bytes());
    /// assert!(SigningKey::<MlDsa44>::from_bytes(&[0u8; 10]).is_err());
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DsaError> {
        if bytes.len() != sk_len::<P>() {
            return Err(DsaError::InvalidLength);
        }
        Ok(Self {
            bytes: bytes.to_vec(),
            _params: PhantomData,
        })
    }

    /// The encoded key. Handle it as a secret.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_pqc::{MlDsa44, SigningKey, dsa_keygen_from_seed};
    ///
    /// let (_vk, sk) = dsa_keygen_from_seed::<MlDsa44>(&[1; 32]);
    /// assert_eq!(sk.as_bytes().len(), SigningKey::<MlDsa44>::encoded_len());
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl<P: Parameters> Drop for SigningKey<P> {
    fn drop(&mut self) {
        wipe(&mut self.bytes);
    }
}

/// A signature for parameter set `P`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature<P: Parameters> {
    bytes: Vec<u8>,
    _params: PhantomData<P>,
}

impl<P: Parameters> Signature<P> {
    /// The encoded length for this parameter set.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_pqc::{MlDsa44, Signature};
    ///
    /// // ML-DSA-44 signatures are 2420 bytes.
    /// assert_eq!(Signature::<MlDsa44>::encoded_len(), 2420);
    /// ```
    pub fn encoded_len() -> usize {
        sig_len::<P>()
    }

    /// Parses a signature, checking its length.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_pqc::{MlDsa44, Signature, dsa_keygen_from_seed, sign_deterministic};
    ///
    /// let (_vk, sk) = dsa_keygen_from_seed::<MlDsa44>(&[1; 32]);
    /// let sig = sign_deterministic(&sk, b"message", b"").unwrap();
    /// let parsed = Signature::<MlDsa44>::from_bytes(sig.as_bytes()).unwrap();
    /// assert_eq!(parsed.as_bytes(), sig.as_bytes());
    /// assert!(Signature::<MlDsa44>::from_bytes(&[0u8; 10]).is_err());
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DsaError> {
        if bytes.len() != sig_len::<P>() {
            return Err(DsaError::InvalidLength);
        }
        Ok(Self {
            bytes: bytes.to_vec(),
            _params: PhantomData,
        })
    }

    /// The encoded signature.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_pqc::{MlDsa44, Signature, dsa_keygen_from_seed, sign_deterministic};
    ///
    /// let (_vk, sk) = dsa_keygen_from_seed::<MlDsa44>(&[1; 32]);
    /// let sig = sign_deterministic(&sk, b"message", b"").unwrap();
    /// assert_eq!(sig.as_bytes().len(), Signature::<MlDsa44>::encoded_len());
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// The decoded secret key: rho, K, tr, s1, s2 and t0.
struct Decoded {
    rho: [u8; 32],
    key: [u8; 32],
    tr: [u8; 64],
    s1: Vec<Poly>,
    s2: Vec<Poly>,
    t0: Vec<Poly>,
}

fn sk_decode<P: Parameters>(bytes: &[u8]) -> Decoded {
    let mut rho = [0u8; 32];
    rho.copy_from_slice(&bytes[..32]);
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes[32..64]);
    let mut tr = [0u8; 64];
    tr.copy_from_slice(&bytes[64..128]);
    let eb = eta_bits::<P>();
    let mut at = 128;
    let mut s1 = Vec::with_capacity(P::L);
    for _ in 0..P::L {
        s1.push(unpack_centered(&bytes[at..at + poly_bytes(eb)], P::ETA, eb));
        at += poly_bytes(eb);
    }
    let mut s2 = Vec::with_capacity(P::K);
    for _ in 0..P::K {
        s2.push(unpack_centered(&bytes[at..at + poly_bytes(eb)], P::ETA, eb));
        at += poly_bytes(eb);
    }
    let mut t0 = Vec::with_capacity(P::K);
    for _ in 0..P::K {
        t0.push(unpack_centered(
            &bytes[at..at + poly_bytes(D)],
            1 << (D - 1),
            D,
        ));
        at += poly_bytes(D);
    }
    Decoded {
        rho,
        key,
        tr,
        s1,
        s2,
        t0,
    }
}

/// ML-DSA.KeyGen_internal from the 32-byte seed `xi`.
///
/// # Examples
///
/// ```
/// use tpt_dev_pqc::{MlDsa44, dsa_keygen_from_seed};
///
/// // The same seed always gives the same key pair.
/// let (vk1, _sk1) = dsa_keygen_from_seed::<MlDsa44>(&[1; 32]);
/// let (vk2, _sk2) = dsa_keygen_from_seed::<MlDsa44>(&[1; 32]);
/// assert_eq!(vk1.as_bytes(), vk2.as_bytes());
/// ```
pub fn keygen_from_seed<P: Parameters>(xi: &[u8; 32]) -> (VerifyingKey<P>, SigningKey<P>) {
    let mut h = [0u8; 128];
    shake256(&[xi, &[P::K as u8], &[P::L as u8]], &mut h);
    let mut rho = [0u8; 32];
    rho.copy_from_slice(&h[..32]);
    let mut rho_prime = [0u8; 64];
    rho_prime.copy_from_slice(&h[32..96]);
    let mut key = [0u8; 32];
    key.copy_from_slice(&h[96..128]);

    let a = expand_a::<P>(&rho);
    let (s1, s2) = expand_s::<P>(&rho_prime);
    let s1_hat: Vec<Poly> = s1
        .iter()
        .map(|p| {
            let mut f = *p;
            ntt(&mut f);
            f
        })
        .collect();

    let mut t1 = Vec::with_capacity(P::K);
    let mut t0 = Vec::with_capacity(P::K);
    for i in 0..P::K {
        let mut acc = ZERO;
        for j in 0..P::L {
            acc = add(&acc, &pointwise(&a[i][j], &s1_hat[j]));
        }
        inverse_ntt(&mut acc);
        let t = add(&acc, &s2[i]);
        let mut hi = ZERO;
        let mut lo = ZERO;
        for c in 0..N {
            let (r1, r0) = power2round(t[c]);
            hi[c] = r1;
            lo[c] = modq(r0);
        }
        t1.push(hi);
        t0.push(lo);
    }

    let mut pk = Vec::with_capacity(pk_len::<P>());
    pk.extend_from_slice(&rho);
    for p in &t1 {
        let values: Vec<u32> = p.iter().map(|&c| c as u32).collect();
        pack(&values, 10, &mut pk);
    }
    let mut tr = [0u8; 64];
    shake256(&[&pk], &mut tr);

    let mut sk = Vec::with_capacity(sk_len::<P>());
    sk.extend_from_slice(&rho);
    sk.extend_from_slice(&key);
    sk.extend_from_slice(&tr);
    let eb = eta_bits::<P>();
    for p in s1.iter().chain(s2.iter()) {
        pack_centered(p, P::ETA, eb, &mut sk);
    }
    for p in &t0 {
        pack_centered(p, 1 << (D - 1), D, &mut sk);
    }

    wipe(&mut h);
    wipe(&mut rho_prime);
    (
        VerifyingKey {
            bytes: pk,
            _params: PhantomData,
        },
        SigningKey {
            bytes: sk,
            _params: PhantomData,
        },
    )
}

/// ML-DSA.Sign_internal on the message representative `m_prime`, with the
/// 32-byte randomizer `rnd`. The deterministic variant uses all zeros.
fn sign_internal<P: Parameters>(sk: &SigningKey<P>, m_prime: &[u8], rnd: &[u8; 32]) -> Vec<u8> {
    let d = sk_decode::<P>(sk.as_bytes());
    let a = expand_a::<P>(&d.rho);
    let s1_hat: Vec<Poly> =
        d.s1.iter()
            .map(|p| {
                let mut f = *p;
                ntt(&mut f);
                f
            })
            .collect();
    let s2_hat: Vec<Poly> =
        d.s2.iter()
            .map(|p| {
                let mut f = *p;
                ntt(&mut f);
                f
            })
            .collect();
    let t0_hat: Vec<Poly> =
        d.t0.iter()
            .map(|p| {
                let mut f = *p;
                ntt(&mut f);
                f
            })
            .collect();

    let mut mu = [0u8; 64];
    shake256(&[&d.tr, m_prime], &mut mu);
    let mut rho_dd = [0u8; 64];
    shake256(&[&d.key, rnd, &mu], &mut rho_dd);

    // Fixed work: every call makes SIGN_ATTEMPTS attempts, whatever the key and
    // message. The first passing attempt is kept by a mask, so the output is the
    // same as the standard "retry until it passes" loop, which is only resumed
    // below if no attempt in the budget passed.
    let attempt =
        |kappa: u16| sign_attempt::<P>(&a, &s1_hat, &s2_hat, &t0_hat, &mu, &rho_dd, kappa);
    let mut out = alloc::vec![0u8; sig_len::<P>()];
    let mut done = 0u32;
    let mut kappa: u16 = 0;
    for _ in 0..SIGN_ATTEMPTS {
        let (candidate, ok) = attempt(kappa);
        kappa = kappa.wrapping_add(P::L as u16);
        let take = ok & (done ^ 1);
        let mask = (take as u8).wrapping_neg();
        for (o, c) in out.iter_mut().zip(&candidate) {
            *o = (*o & !mask) | (*c & mask);
        }
        done |= ok;
    }
    if done == 0 {
        // Rare: the budget ran out. The standard loop continues from where the
        // budget stopped, so the result is the same as an unbounded loop.
        loop {
            let (candidate, ok) = attempt(kappa);
            kappa = kappa.wrapping_add(P::L as u16);
            if ok == 1 {
                out = candidate;
                break;
            }
        }
    }
    wipe(&mut rho_dd);
    out
}

/// Attempts a signature with masking counter `kappa`. Returns the encoded
/// signature, and 1 if it passed every rejection check, else 0. The encoding is
/// built whatever the outcome, so the work does not depend on it.
fn sign_attempt<P: Parameters>(
    a: &[Vec<Poly>],
    s1_hat: &[Poly],
    s2_hat: &[Poly],
    t0_hat: &[Poly],
    mu: &[u8; 64],
    rho_dd: &[u8; 64],
    kappa: u16,
) -> (Vec<u8>, u32) {
    let y = expand_mask::<P>(rho_dd, kappa);
    let mut y_hat = y.clone();
    for p in y_hat.iter_mut() {
        ntt(p);
    }

    // w = A y, in the normal domain.
    let mut w = Vec::with_capacity(P::K);
    for row in a {
        let mut acc = ZERO;
        for (ar, yh) in row.iter().zip(&y_hat) {
            acc = add(&acc, &pointwise(ar, yh));
        }
        inverse_ntt(&mut acc);
        w.push(acc);
    }
    let w1: Vec<Poly> = w
        .iter()
        .map(|p| {
            let mut hi = ZERO;
            for c in 0..N {
                hi[c] = high_bits(p[c], P::GAMMA2);
            }
            hi
        })
        .collect();

    let mut c_tilde = alloc::vec![0u8; P::CTILDE];
    shake256(&[mu, &w1_encode::<P>(&w1)], &mut c_tilde);
    let mut c_hat = sample_in_ball(&c_tilde, P::TAU);
    ntt(&mut c_hat);

    // z = y + c s1
    let mut z = Vec::with_capacity(P::L);
    for j in 0..P::L {
        let mut cs1 = pointwise(&c_hat, &s1_hat[j]);
        inverse_ntt(&mut cs1);
        z.push(add(&y[j], &cs1));
    }
    // w - c s2, whose low bits must stay small.
    let mut w_minus_cs2 = Vec::with_capacity(P::K);
    for (wi, s2) in w.iter().zip(s2_hat) {
        let mut cs2 = pointwise(&c_hat, s2);
        inverse_ntt(&mut cs2);
        w_minus_cs2.push(sub(wi, &cs2));
    }

    // Rejection checks, combined with masks: every check runs to the end.
    let beta = P::ETA * P::TAU as i32;
    let mut ok = centred_within(&z, P::GAMMA1 - beta);
    ok &= low_within(&w_minus_cs2, P::GAMMA2, P::GAMMA2 - beta);

    // ct0, and the hint for the high bits.
    let mut ct0 = Vec::with_capacity(P::K);
    for t0 in t0_hat {
        let mut p = pointwise(&c_hat, t0);
        inverse_ntt(&mut p);
        ct0.push(p);
    }
    ok &= centred_within(&ct0, P::GAMMA2);
    let mut h = Vec::with_capacity(P::K);
    let mut ones: i32 = 0;
    for i in 0..P::K {
        let mut row = ZERO;
        let restored = add(&w_minus_cs2[i], &ct0[i]);
        for c in 0..N {
            let bit = make_hint(-ct0[i][c], restored[c], P::GAMMA2);
            row[c] = bit;
            ones += bit;
        }
        h.push(row);
    }
    // Too many hint bits: ones > OMEGA makes the sign of OMEGA - ones negative.
    let too_many = (((P::OMEGA as i32) - ones) >> 31) as u32 & 1;
    ok &= too_many ^ 1;

    let mut sigma = Vec::with_capacity(sig_len::<P>());
    sigma.extend_from_slice(&c_tilde);
    for p in &z {
        pack_centered(p, P::GAMMA1, z_bits::<P>(), &mut sigma);
    }
    sigma.extend_from_slice(&hint_pack::<P>(&h));
    (sigma, ok)
}

/// The message representative for pure signing: `0 || len(ctx) || ctx || msg`.
fn pure_message(msg: &[u8], ctx: &[u8]) -> Result<Vec<u8>, DsaError> {
    if ctx.len() > 255 {
        return Err(DsaError::ContextTooLong);
    }
    let mut m = Vec::with_capacity(2 + ctx.len() + msg.len());
    m.push(0);
    m.push(ctx.len() as u8);
    m.extend_from_slice(ctx);
    m.extend_from_slice(msg);
    Ok(m)
}

/// Signs `msg` with context `ctx`, using the randomizer `rnd`. Pass all zeros for
/// the deterministic variant.
fn sign_with_randomizer<P: Parameters>(
    sk: &SigningKey<P>,
    msg: &[u8],
    ctx: &[u8],
    rnd: &[u8; 32],
) -> Result<Signature<P>, DsaError> {
    let m_prime = pure_message(msg, ctx)?;
    let bytes = sign_internal(sk, &m_prime, rnd);
    Ok(Signature {
        bytes,
        _params: PhantomData,
    })
}

/// Deterministic signing: the same key and message always give the same signature.
///
/// # Examples
///
/// ```
/// use tpt_dev_pqc::{MlDsa44, dsa_keygen_from_seed, sign_deterministic, verify};
///
/// let (vk, sk) = dsa_keygen_from_seed::<MlDsa44>(&[1; 32]);
/// let sig = sign_deterministic(&sk, b"hello", b"ctx").unwrap();
/// assert!(verify(&vk, b"hello", b"ctx", &sig));
/// ```
pub fn sign_deterministic<P: Parameters>(
    sk: &SigningKey<P>,
    msg: &[u8],
    ctx: &[u8],
) -> Result<Signature<P>, DsaError> {
    sign_with_randomizer(sk, msg, ctx, &[0u8; 32])
}

/// Hedged signing: a fresh 32-byte randomizer is drawn from `rng`, which makes
/// each signature differ even for the same message.
///
/// # Examples
///
/// ```
/// struct Counter(u8);
/// impl Rng for Counter {
///     fn fill_bytes(&mut self, dest: &mut [u8]) {
///         for b in dest.iter_mut() {
///             *b = self.0;
///             self.0 = self.0.wrapping_add(1);
///         }
///     }
/// }
/// use tpt_dev_pqc::{MlDsa44, Rng, dsa_keygen_from_seed, dsa_sign, verify};
///
/// let (vk, sk) = dsa_keygen_from_seed::<MlDsa44>(&[1; 32]);
/// // Hedged signing: the randomizer comes from the generator, so each signature differs.
/// let mut rng = Counter(0);
/// let sig = dsa_sign(&sk, b"hello", b"", &mut rng).unwrap();
/// assert!(verify(&vk, b"hello", b"", &sig));
/// ```
pub fn sign<P: Parameters, R: Rng>(
    sk: &SigningKey<P>,
    msg: &[u8],
    ctx: &[u8],
    rng: &mut R,
) -> Result<Signature<P>, DsaError> {
    let mut rnd = [0u8; 32];
    rng.fill_bytes(&mut rnd);
    let out = sign_with_randomizer(sk, msg, ctx, &rnd);
    wipe(&mut rnd);
    out
}

/// ML-DSA.KeyGen with the seed drawn from `rng`.
///
/// # Examples
///
/// ```
/// struct Counter(u8);
/// impl Rng for Counter {
///     fn fill_bytes(&mut self, dest: &mut [u8]) {
///         for b in dest.iter_mut() {
///             *b = self.0;
///             self.0 = self.0.wrapping_add(1);
///         }
///     }
/// }
/// use tpt_dev_pqc::{MlDsa44, Rng, dsa_keygen, sign_deterministic, verify};
///
/// let mut rng = Counter(0);
/// let (vk, sk) = dsa_keygen::<MlDsa44, _>(&mut rng);
/// let sig = sign_deterministic(&sk, b"hello", b"").unwrap();
/// assert!(verify(&vk, b"hello", b"", &sig));
/// ```
pub fn keygen<P: Parameters, R: Rng>(rng: &mut R) -> (VerifyingKey<P>, SigningKey<P>) {
    let mut xi = [0u8; 32];
    rng.fill_bytes(&mut xi);
    let keys = keygen_from_seed(&xi);
    wipe(&mut xi);
    keys
}

/// ML-DSA.Verify. Returns `true` only for a valid signature on `msg` under `ctx`.
///
/// # Examples
///
/// ```
/// use tpt_dev_pqc::{MlDsa44, dsa_keygen_from_seed, sign_deterministic, verify};
///
/// let (vk, sk) = dsa_keygen_from_seed::<MlDsa44>(&[9; 32]);
/// let sig = sign_deterministic(&sk, b"msg", b"").unwrap();
/// assert!(verify(&vk, b"msg", b"", &sig));
/// // A different message does not verify.
/// assert!(!verify(&vk, b"other", b"", &sig));
/// ```
pub fn verify<P: Parameters>(
    vk: &VerifyingKey<P>,
    msg: &[u8],
    ctx: &[u8],
    sig: &Signature<P>,
) -> bool {
    let Ok(m_prime) = pure_message(msg, ctx) else {
        return false;
    };
    let pk = vk.as_bytes();
    let mut rho = [0u8; 32];
    rho.copy_from_slice(&pk[..32]);
    let eb = 10u32;
    let t1: Vec<Poly> = (0..P::K)
        .map(|i| {
            let start = 32 + i * poly_bytes(eb);
            let values = unpack(&pk[start..start + poly_bytes(eb)], eb, N);
            let mut p = ZERO;
            for (c, v) in p.iter_mut().zip(values) {
                *c = v as i32;
            }
            p
        })
        .collect();

    let sig_bytes = sig.as_bytes();
    let c_tilde = &sig_bytes[..P::CTILDE];
    let zb = z_bits::<P>();
    let z_start = P::CTILDE;
    let z_end = z_start + P::L * poly_bytes(zb);
    let z: Vec<Poly> = (0..P::L)
        .map(|j| {
            let start = z_start + j * poly_bytes(zb);
            unpack_centered(&sig_bytes[start..start + poly_bytes(zb)], P::GAMMA1, zb)
        })
        .collect();
    let Some(h) = hint_unpack::<P>(&sig_bytes[z_end..]) else {
        return false;
    };

    let mut tr = [0u8; 64];
    shake256(&[pk], &mut tr);
    let mut mu = [0u8; 64];
    shake256(&[&tr, &m_prime], &mut mu);

    let a = expand_a::<P>(&rho);
    let mut z_hat = z.clone();
    for p in z_hat.iter_mut() {
        ntt(p);
    }
    let mut c_hat = sample_in_ball(c_tilde, P::TAU);
    ntt(&mut c_hat);
    let t1_hat: Vec<Poly> = t1
        .iter()
        .map(|p| {
            let mut scaled = ZERO;
            for c in 0..N {
                scaled[c] = modq(p[c] << D);
            }
            ntt(&mut scaled);
            scaled
        })
        .collect();

    let mut w1 = Vec::with_capacity(P::K);
    for i in 0..P::K {
        let mut acc = ZERO;
        for j in 0..P::L {
            acc = add(&acc, &pointwise(&a[i][j], &z_hat[j]));
        }
        let ct1 = pointwise(&c_hat, &t1_hat[i]);
        let mut w_prime = sub(&acc, &ct1);
        inverse_ntt(&mut w_prime);
        let mut hi = ZERO;
        for c in 0..N {
            hi[c] = use_hint(h[i][c] != 0, w_prime[c], P::GAMMA2);
        }
        w1.push(hi);
    }

    let mut c_check = alloc::vec![0u8; P::CTILDE];
    shake256(&[&mu, &w1_encode::<P>(&w1)], &mut c_check);

    let beta = P::ETA * P::TAU as i32;
    let z_ok = z
        .iter()
        .all(|p| p.iter().all(|&c| center(c).abs() < P::GAMMA1 - beta));
    // Every input here is public, so an ordinary comparison is fine.
    z_ok && c_tilde == c_check.as_slice()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn ntt_round_trips() {
        let mut f = ZERO;
        for (i, c) in f.iter_mut().enumerate() {
            *c = modq((i as i32) * 7919 + 3);
        }
        let original = f;
        ntt(&mut f);
        inverse_ntt(&mut f);
        assert_eq!(f, original);
    }

    #[test]
    fn power2round_recombines() {
        for r in [0, 1, 4095, 4096, 4097, Q - 1, 123_456] {
            let (r1, r0) = power2round(r);
            assert_eq!(modq(r1 * (1 << D) + r0), modq(r));
        }
    }

    #[test]
    fn decompose_recombines_and_bounds_the_low_part() {
        let g2 = (Q - 1) / 88;
        for r in [0, 1, g2, 2 * g2, Q - 1, 5_000_000] {
            let (r1, r0) = decompose(r, g2);
            assert!(r0.abs() <= g2);
            assert_eq!(modq(r1 * 2 * g2 + r0), modq(r));
        }
    }

    #[test]
    fn hint_packing_round_trips() {
        let mut h = alloc::vec![ZERO; MlDsa44::K];
        h[0][3] = 1;
        h[0][200] = 1;
        h[2][0] = 1;
        let packed = hint_pack::<MlDsa44>(&h);
        assert_eq!(hint_unpack::<MlDsa44>(&packed).unwrap(), h);
    }

    #[test]
    fn a_malformed_hint_is_rejected() {
        let mut packed = alloc::vec![0u8; MlDsa44::OMEGA + MlDsa44::K];
        packed[MlDsa44::OMEGA] = 5;
        // A count larger than the positions that follow it is malformed.
        packed[MlDsa44::OMEGA + 1] = 1;
        assert!(hint_unpack::<MlDsa44>(&packed).is_none());
    }

    #[test]
    fn challenge_has_tau_nonzero_coefficients() {
        let c = sample_in_ball(&[7u8; 32], MlDsa44::TAU);
        let nonzero = c.iter().filter(|&&x| x != 0).count();
        assert_eq!(nonzero, MlDsa44::TAU);
        assert!(c.iter().all(|&x| x == 0 || x == 1 || x == Q - 1));
    }
}

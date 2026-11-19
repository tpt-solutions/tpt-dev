//! Polynomials in the ring Z_q[X]/(X^256 + 1), with q = 3329.
//!
//! This module has the number-theoretic transform, the compression and byte
//! encodings of FIPS 203, and the samplers that draw matrices and noise from
//! hash output. Coefficients are stored reduced to `0..Q`.

use alloc::vec::Vec;

use crate::keccak::{Sponge, shake256};

/// The modulus q.
pub(crate) const Q: u32 = 3329;

/// A polynomial: 256 coefficients in `0..Q`.
pub(crate) type Poly = [u16; 256];

/// The zero polynomial.
pub(crate) const ZERO: Poly = [0; 256];

const fn pow_mod(mut base: u32, mut exp: u32) -> u32 {
    let mut r = 1;
    while exp > 0 {
        if exp & 1 == 1 {
            r = r * base % Q;
        }
        base = base * base % Q;
        exp >>= 1;
    }
    r
}

const fn bit_reverse7(x: u32) -> u32 {
    let mut r = 0;
    let mut i = 0;
    while i < 7 {
        r |= ((x >> i) & 1) << (6 - i);
        i += 1;
    }
    r
}

/// zeta^BitRev7(i), with zeta = 17, the twiddle factors of the NTT.
const ZETAS: [u16; 128] = {
    let mut t = [0u16; 128];
    let mut i = 0;
    while i < 128 {
        t[i] = pow_mod(17, bit_reverse7(i as u32)) as u16;
        i += 1;
    }
    t
};

/// 17^(2 * BitRev7(i) + 1), the factors used when multiplying in the NTT domain.
const GAMMAS: [u16; 128] = {
    let mut t = [0u16; 128];
    let mut i = 0;
    while i < 128 {
        t[i] = pow_mod(17, 2 * bit_reverse7(i as u32) + 1) as u16;
        i += 1;
    }
    t
};

/// 128^-1 mod q, which scales the inverse NTT.
const N_INV: u32 = 3303;

/// Adds two polynomials coefficient by coefficient.
pub(crate) fn add(a: &Poly, b: &Poly) -> Poly {
    let mut out = ZERO;
    for i in 0..256 {
        out[i] = ((u32::from(a[i]) + u32::from(b[i])) % Q) as u16;
    }
    out
}

/// Subtracts `b` from `a` coefficient by coefficient.
pub(crate) fn sub(a: &Poly, b: &Poly) -> Poly {
    let mut out = ZERO;
    for i in 0..256 {
        out[i] = ((u32::from(a[i]) + Q - u32::from(b[i])) % Q) as u16;
    }
    out
}

/// The forward number-theoretic transform, in place.
pub(crate) fn ntt(f: &mut Poly) {
    let mut k = 1;
    let mut len = 128;
    while len >= 2 {
        let mut start = 0;
        while start < 256 {
            let zeta = u32::from(ZETAS[k]);
            k += 1;
            for j in start..start + len {
                let t = zeta * u32::from(f[j + len]) % Q;
                let a = u32::from(f[j]);
                f[j + len] = ((a + Q - t) % Q) as u16;
                f[j] = ((a + t) % Q) as u16;
            }
            start += 2 * len;
        }
        len /= 2;
    }
}

/// The inverse number-theoretic transform, in place.
pub(crate) fn inverse_ntt(f: &mut Poly) {
    let mut k = 127;
    let mut len = 2;
    while len <= 128 {
        let mut start = 0;
        while start < 256 {
            let zeta = u32::from(ZETAS[k]);
            k -= 1;
            for j in start..start + len {
                let t = u32::from(f[j]);
                let b = u32::from(f[j + len]);
                f[j] = ((t + b) % Q) as u16;
                f[j + len] = (zeta * ((b + Q - t) % Q) % Q) as u16;
            }
            start += 2 * len;
        }
        len *= 2;
    }
    for c in f.iter_mut() {
        *c = (u32::from(*c) * N_INV % Q) as u16;
    }
}

/// Multiplies two polynomials that are in the NTT domain.
pub(crate) fn multiply_ntt(a: &Poly, b: &Poly) -> Poly {
    let mut out = ZERO;
    for i in 0..128 {
        let (a0, a1) = (u32::from(a[2 * i]), u32::from(a[2 * i + 1]));
        let (b0, b1) = (u32::from(b[2 * i]), u32::from(b[2 * i + 1]));
        let gamma = u32::from(GAMMAS[i]);
        out[2 * i] = ((a0 * b0 + a1 * b1 % Q * gamma) % Q) as u16;
        out[2 * i + 1] = ((a0 * b1 + a1 * b0) % Q) as u16;
    }
    out
}

/// Compress_d: rounds each coefficient to a `d`-bit value, scaled by 2^d / q.
pub(crate) fn compress(f: &Poly, d: u32) -> Poly {
    let mask = (1u32 << d) - 1;
    let mut out = ZERO;
    for i in 0..256 {
        let x = u32::from(f[i]);
        out[i] = ((((x << d) + Q / 2) / Q) & mask) as u16;
    }
    out
}

/// Decompress_d: the inverse of [`compress`], up to rounding.
pub(crate) fn decompress(f: &Poly, d: u32) -> Poly {
    let mut out = ZERO;
    for i in 0..256 {
        let y = u32::from(f[i]);
        out[i] = (((y * Q) + (1 << (d - 1))) >> d) as u16;
    }
    out
}

/// ByteEncode_d: packs 256 `d`-bit values into `32 * d` bytes, least significant bit first.
pub(crate) fn byte_encode(f: &Poly, d: u32, out: &mut [u8]) {
    let mut acc = 0u32;
    let mut bits = 0u32;
    let mut idx = 0;
    for &c in f.iter() {
        acc |= u32::from(c) << bits;
        bits += d;
        while bits >= 8 {
            out[idx] = acc as u8;
            idx += 1;
            acc >>= 8;
            bits -= 8;
        }
    }
}

/// ByteDecode_d without reducing. A 12-bit value may be `Q` or more, which a
/// caller must reject where the specification requires it.
pub(crate) fn byte_decode_raw(bytes: &[u8], d: u32) -> Poly {
    let mask = (1u32 << d) - 1;
    let mut out = ZERO;
    let mut acc = 0u32;
    let mut bits = 0u32;
    let mut idx = 0;
    for c in out.iter_mut() {
        while bits < d {
            acc |= u32::from(bytes[idx]) << bits;
            idx += 1;
            bits += 8;
        }
        *c = (acc & mask) as u16;
        acc >>= d;
        bits -= d;
    }
    out
}

/// ByteDecode_d for `d` below 12, where every value is already reduced.
pub(crate) fn byte_decode(bytes: &[u8], d: u32) -> Poly {
    byte_decode_raw(bytes, d)
}

/// ByteDecode_12 with the reduction the specification applies.
pub(crate) fn byte_decode12(bytes: &[u8]) -> Poly {
    let mut p = byte_decode_raw(bytes, 12);
    for c in p.iter_mut() {
        *c = (u32::from(*c) % Q) as u16;
    }
    p
}

/// Whether every 12-bit value in `bytes` is below `Q`.
pub(crate) fn all_below_q(bytes: &[u8]) -> bool {
    byte_decode_raw(bytes, 12).iter().all(|&c| u32::from(c) < Q)
}

/// SampleNTT: a uniform polynomial in the NTT domain, from SHAKE128 of `rho || x || y`.
pub(crate) fn sample_ntt(rho: &[u8; 32], x: u8, y: u8) -> Poly {
    let mut xof = Sponge::new(168);
    xof.absorb(rho);
    xof.absorb(&[x, y]);
    xof.finish(0x1f);
    let mut f = ZERO;
    let mut n = 0;
    let mut buf = [0u8; 3];
    while n < 256 {
        xof.squeeze(&mut buf);
        let d1 = u32::from(buf[0]) | ((u32::from(buf[1]) & 0x0f) << 8);
        let d2 = (u32::from(buf[1]) >> 4) | (u32::from(buf[2]) << 4);
        if d1 < Q {
            f[n] = d1 as u16;
            n += 1;
        }
        if n < 256 && d2 < Q {
            f[n] = d2 as u16;
            n += 1;
        }
    }
    f
}

/// SamplePolyCBD_eta: a polynomial with small coefficients, from `64 * eta` random bytes.
pub(crate) fn sample_cbd(bytes: &[u8], eta: usize) -> Poly {
    let bit = |i: usize| u32::from((bytes[i / 8] >> (i % 8)) & 1);
    let mut f = ZERO;
    for (i, c) in f.iter_mut().enumerate() {
        let mut x = 0;
        let mut y = 0;
        for j in 0..eta {
            x += bit(2 * i * eta + j);
            y += bit(2 * i * eta + eta + j);
        }
        *c = ((x + Q - y) % Q) as u16;
    }
    f
}

/// PRF_eta(s, b): `64 * eta` bytes of SHAKE256 of `s || b`.
pub(crate) fn prf(s: &[u8; 32], b: u8, out: &mut [u8]) {
    shake256(&[s, &[b]], out);
}

/// Encodes a vector of polynomials, `d` bits per coefficient.
pub(crate) fn encode_vector(v: &[Poly], d: u32) -> Vec<u8> {
    let mut out = alloc::vec![0u8; v.len() * 32 * d as usize];
    for (i, p) in v.iter().enumerate() {
        let start = i * 32 * d as usize;
        byte_encode(p, d, &mut out[start..start + 32 * d as usize]);
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample_poly(seed: u32) -> Poly {
        let mut p = ZERO;
        let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
        for c in p.iter_mut() {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            *c = ((x >> 8) % Q) as u16;
        }
        p
    }

    #[test]
    fn ntt_round_trips() {
        for seed in 0..20 {
            let original = sample_poly(seed);
            let mut f = original;
            ntt(&mut f);
            inverse_ntt(&mut f);
            assert_eq!(f, original);
        }
    }

    #[test]
    fn ntt_product_matches_schoolbook_negacyclic_product() {
        let a = sample_poly(3);
        let b = sample_poly(7);
        // Schoolbook multiplication modulo X^256 + 1.
        let mut expected = [0u32; 256];
        for i in 0..256 {
            for j in 0..256 {
                let term = u32::from(a[i]) * u32::from(b[j]) % Q;
                if i + j < 256 {
                    expected[i + j] = (expected[i + j] + term) % Q;
                } else {
                    expected[i + j - 256] = (expected[i + j - 256] + Q - term) % Q;
                }
            }
        }
        let (mut fa, mut fb) = (a, b);
        ntt(&mut fa);
        ntt(&mut fb);
        let mut prod = multiply_ntt(&fa, &fb);
        inverse_ntt(&mut prod);
        for i in 0..256 {
            assert_eq!(u32::from(prod[i]), expected[i], "coefficient {i}");
        }
    }

    #[test]
    fn byte_encoding_round_trips_for_every_width() {
        for d in [1u32, 4, 5, 10, 11, 12] {
            let p = compress(&sample_poly(9), d);
            let mut bytes = alloc::vec![0u8; 32 * d as usize];
            byte_encode(&p, d, &mut bytes);
            assert_eq!(byte_decode(&bytes, d), p, "d = {d}");
        }
    }

    #[test]
    fn compression_error_is_within_rounding() {
        // Decompress(Compress(x)) is within half a step of x, for every x.
        for x in 0..Q as u16 {
            let mut p = ZERO;
            p[0] = x;
            let back = decompress(&compress(&p, 10), 10)[0];
            let diff = (i32::from(back) - i32::from(x)).rem_euclid(Q as i32);
            let dist = diff.min(Q as i32 - diff);
            assert!(dist <= (Q as i32 + 1023) / 1024, "x = {x}, back = {back}");
        }
    }

    #[test]
    fn cbd_coefficients_are_small() {
        let bytes = [0xa5u8; 128];
        let p = sample_cbd(&bytes, 2);
        for &c in p.iter() {
            let signed = if u32::from(c) > Q / 2 {
                i32::from(c) - Q as i32
            } else {
                i32::from(c)
            };
            assert!(signed.abs() <= 2);
        }
    }
}

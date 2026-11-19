//! ML-KEM (FIPS 203): a key-encapsulation mechanism from module lattices.
//!
//! The scheme has three layers. K-PKE is the underlying public-key encryption
//! scheme. ML-KEM wraps it with a random message and a hash of the keys, which
//! turns it into a key-encapsulation mechanism. Decapsulation uses implicit
//! rejection: a bad ciphertext yields a pseudo-random secret, not an error. The
//! selection between the two secrets uses masks, not branches.

use alloc::vec::Vec;
use core::marker::PhantomData;

use crate::keccak::{sha3_256, sha3_512, shake256};
use crate::poly::{
    Poly, ZERO, add, all_below_q, byte_decode, byte_decode12, byte_encode, compress, decompress,
    encode_vector, inverse_ntt, multiply_ntt, ntt, prf, sample_cbd, sample_ntt, sub,
};

/// Length of a shared secret, in bytes.
pub const SHARED_SECRET_LEN: usize = 32;

/// The noise parameter used for the encryption randomness in the second
/// position. It is 2 in all three parameter sets.
const ETA2: usize = 2;

/// Parameters of one ML-KEM security level.
pub trait Parameters: Clone + core::fmt::Debug + PartialEq + Eq {
    /// Rank of the module: the number of polynomials per vector.
    const K: usize;
    /// Noise parameter for the secret and the first error vector.
    const ETA1: usize;
    /// Bits kept per coefficient of the first ciphertext component.
    const DU: u32;
    /// Bits kept per coefficient of the second ciphertext component.
    const DV: u32;
    /// The level's name, for messages.
    const NAME: &'static str;
}

/// ML-KEM-512, NIST security category 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MlKem512;

/// ML-KEM-768, NIST security category 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MlKem768;

/// ML-KEM-1024, NIST security category 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MlKem1024;

impl Parameters for MlKem512 {
    const K: usize = 2;
    const ETA1: usize = 3;
    const DU: u32 = 10;
    const DV: u32 = 4;
    const NAME: &'static str = "ML-KEM-512";
}

impl Parameters for MlKem768 {
    const K: usize = 3;
    const ETA1: usize = 2;
    const DU: u32 = 10;
    const DV: u32 = 4;
    const NAME: &'static str = "ML-KEM-768";
}

impl Parameters for MlKem1024 {
    const K: usize = 4;
    const ETA1: usize = 2;
    const DU: u32 = 11;
    const DV: u32 = 5;
    const NAME: &'static str = "ML-KEM-1024";
}

/// A source of random bytes supplied by the caller.
///
/// The crate does not provide randomness. A caller passes a cryptographically
/// secure generator, and it must not fail silently. This trait is deliberately
/// not tied to any RNG crate, so it works without `std`.
pub trait Rng {
    /// Fills `dest` with random bytes.
    fn fill_bytes(&mut self, dest: &mut [u8]);
}

/// Why a key or ciphertext could not be accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KemError {
    /// The byte string has the wrong length for this parameter set.
    InvalidLength,
    /// The encapsulation key holds a coefficient that is not reduced modulo q.
    InvalidEncapsulationKey,
}

impl core::fmt::Display for KemError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            KemError::InvalidLength => "wrong length for this ML-KEM parameter set",
            KemError::InvalidEncapsulationKey => "encapsulation key fails the modulus check",
        })
    }
}

/// Overwrites `bytes` with zeros. The volatile-like barrier keeps the compiler
/// from dropping the writes as dead stores. This is best effort, not a guarantee.
fn wipe(bytes: &mut [u8]) {
    for b in bytes.iter_mut() {
        *b = 0;
    }
    core::hint::black_box(bytes);
}

fn seed_from(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes[..32]);
    out
}

/// The matrix A, or its transpose, drawn from `rho`. Entry (i, j) of A is
/// SampleNTT(rho || j || i), so the transpose swaps the indices.
fn gen_matrix(rho: &[u8; 32], k: usize, transpose: bool) -> Vec<Vec<Poly>> {
    (0..k)
        .map(|i| {
            (0..k)
                .map(|j| {
                    if transpose {
                        sample_ntt(rho, i as u8, j as u8)
                    } else {
                        sample_ntt(rho, j as u8, i as u8)
                    }
                })
                .collect()
        })
        .collect()
}

/// Sum over j of row[j] times vec[j], in the NTT domain.
fn inner_product(row: &[Poly], vec: &[Poly]) -> Poly {
    let mut acc = ZERO;
    for (a, b) in row.iter().zip(vec) {
        acc = add(&acc, &multiply_ntt(a, b));
    }
    acc
}

/// Noise drawn from PRF_eta, with the counter `n`.
fn noise(sigma: &[u8; 32], n: u8, eta: usize) -> Poly {
    let mut buf = [0u8; 192];
    prf(sigma, n, &mut buf[..64 * eta]);
    let p = sample_cbd(&buf[..64 * eta], eta);
    wipe(&mut buf);
    p
}

/// K-PKE.KeyGen. Returns the encryption key and the decryption key.
fn k_pke_keygen(d: &[u8; 32], k: usize, eta1: usize) -> (Vec<u8>, Vec<u8>) {
    let g = sha3_512(&[d, &[k as u8]]);
    let rho = seed_from(&g[..32]);
    let sigma = seed_from(&g[32..]);
    let a = gen_matrix(&rho, k, false);

    let mut n = 0u8;
    let mut s = Vec::with_capacity(k);
    for _ in 0..k {
        s.push(noise(&sigma, n, eta1));
        n += 1;
    }
    let mut e = Vec::with_capacity(k);
    for _ in 0..k {
        e.push(noise(&sigma, n, eta1));
        n += 1;
    }
    for p in s.iter_mut().chain(e.iter_mut()) {
        ntt(p);
    }
    let s_hat = s;
    let t_hat: Vec<Poly> = (0..k)
        .map(|i| add(&inner_product(&a[i], &s_hat), &e[i]))
        .collect();

    let mut ek = encode_vector(&t_hat, 12);
    ek.extend_from_slice(&rho);
    let dk = encode_vector(&s_hat, 12);
    (ek, dk)
}

/// K-PKE.Encrypt. The message is 32 bytes, and the randomness `r` is 32 bytes.
fn k_pke_encrypt(
    ek: &[u8],
    m: &[u8; 32],
    r: &[u8; 32],
    k: usize,
    eta1: usize,
    du: u32,
    dv: u32,
) -> Vec<u8> {
    let t_hat: Vec<Poly> = (0..k)
        .map(|i| byte_decode12(&ek[384 * i..384 * (i + 1)]))
        .collect();
    let rho = seed_from(&ek[384 * k..384 * k + 32]);
    let at = gen_matrix(&rho, k, true);

    let mut n = 0u8;
    let mut y = Vec::with_capacity(k);
    for _ in 0..k {
        y.push(noise(r, n, eta1));
        n += 1;
    }
    let mut e1 = Vec::with_capacity(k);
    for _ in 0..k {
        e1.push(noise(r, n, ETA2));
        n += 1;
    }
    let e2 = noise(r, n, ETA2);

    for p in y.iter_mut() {
        ntt(p);
    }
    let y_hat = y;

    let mut u = Vec::with_capacity(k);
    for i in 0..k {
        let mut p = inner_product(&at[i], &y_hat);
        inverse_ntt(&mut p);
        u.push(add(&p, &e1[i]));
    }

    let mut v = inner_product(&t_hat, &y_hat);
    inverse_ntt(&mut v);
    let mu = decompress(&byte_decode(m, 1), 1);
    let v = add(&add(&v, &e2), &mu);

    let mut c = Vec::new();
    for p in &u {
        c.extend(encode_vector(&[compress(p, du)], du));
    }
    c.extend(encode_vector(&[compress(&v, dv)], dv));
    c
}

/// K-PKE.Decrypt. Returns the 32-byte message.
fn k_pke_decrypt(dk: &[u8], c: &[u8], k: usize, du: u32, dv: u32) -> [u8; 32] {
    let u_len = 32 * du as usize;
    let u: Vec<Poly> = (0..k)
        .map(|i| decompress(&byte_decode(&c[u_len * i..u_len * (i + 1)], du), du))
        .collect();
    let v_start = u_len * k;
    let v = decompress(&byte_decode(&c[v_start..], dv), dv);

    let s_hat: Vec<Poly> = (0..k)
        .map(|i| byte_decode12(&dk[384 * i..384 * (i + 1)]))
        .collect();
    let u_hat: Vec<Poly> = u
        .into_iter()
        .map(|mut p| {
            ntt(&mut p);
            p
        })
        .collect();
    let mut inner = inner_product(&s_hat, &u_hat);
    inverse_ntt(&mut inner);
    let w = sub(&v, &inner);

    let mut m = [0u8; 32];
    byte_encode(&compress(&w, 1), 1, &mut m);
    m
}

/// A mask that is all ones when `a == b`, and zero otherwise. It does not branch on the data.
fn equal_mask(a: &[u8], b: &[u8]) -> u8 {
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    let is_zero = (u32::from(diff).wrapping_sub(1) >> 31) as u8;
    0u8.wrapping_sub(is_zero)
}

/// An encapsulation (public) key for parameter set `P`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncapsulationKey<P: Parameters> {
    bytes: Vec<u8>,
    _params: PhantomData<P>,
}

impl<P: Parameters> EncapsulationKey<P> {
    /// The encoded length for this parameter set.
    pub fn encoded_len() -> usize {
        384 * P::K + 32
    }

    /// Parses an encapsulation key, checking its length and its modulus.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KemError> {
        if bytes.len() != Self::encoded_len() {
            return Err(KemError::InvalidLength);
        }
        for i in 0..P::K {
            if !all_below_q(&bytes[384 * i..384 * (i + 1)]) {
                return Err(KemError::InvalidEncapsulationKey);
            }
        }
        Ok(Self {
            bytes: bytes.to_vec(),
            _params: PhantomData,
        })
    }

    /// The encoded key.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A decapsulation (secret) key for parameter set `P`. It is wiped when dropped,
/// on a best-effort basis. It has no `Debug` or `Clone`, so it is not copied or printed.
pub struct DecapsulationKey<P: Parameters> {
    bytes: Vec<u8>,
    _params: PhantomData<P>,
}

impl<P: Parameters> DecapsulationKey<P> {
    /// The encoded length for this parameter set.
    pub fn encoded_len() -> usize {
        768 * P::K + 96
    }

    /// Parses a decapsulation key, checking only its length.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KemError> {
        if bytes.len() != Self::encoded_len() {
            return Err(KemError::InvalidLength);
        }
        Ok(Self {
            bytes: bytes.to_vec(),
            _params: PhantomData,
        })
    }

    /// The encoded key. Handle it as a secret.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl<P: Parameters> Drop for DecapsulationKey<P> {
    fn drop(&mut self) {
        wipe(&mut self.bytes);
    }
}

/// A ciphertext for parameter set `P`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ciphertext<P: Parameters> {
    bytes: Vec<u8>,
    _params: PhantomData<P>,
}

impl<P: Parameters> Ciphertext<P> {
    /// The encoded length for this parameter set.
    pub fn encoded_len() -> usize {
        32 * (P::DU as usize * P::K + P::DV as usize)
    }

    /// Parses a ciphertext, checking its length.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KemError> {
        if bytes.len() != Self::encoded_len() {
            return Err(KemError::InvalidLength);
        }
        Ok(Self {
            bytes: bytes.to_vec(),
            _params: PhantomData,
        })
    }

    /// The encoded ciphertext.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A shared secret. It is wiped when dropped, on a best-effort basis.
pub struct SharedSecret([u8; SHARED_SECRET_LEN]);

impl SharedSecret {
    /// The 32 bytes of the secret.
    pub fn as_bytes(&self) -> &[u8; SHARED_SECRET_LEN] {
        &self.0
    }
}

impl Drop for SharedSecret {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// ML-KEM.KeyGen from the two 32-byte seeds `d` and `z`. This is the
/// deterministic core; [`keygen`] draws the seeds from an [`Rng`].
pub fn keygen_from_seed<P: Parameters>(
    d: &[u8; 32],
    z: &[u8; 32],
) -> (EncapsulationKey<P>, DecapsulationKey<P>) {
    let (ek, mut dk_pke) = k_pke_keygen(d, P::K, P::ETA1);
    let h = sha3_256(&[&ek]);
    let mut dk = Vec::with_capacity(DecapsulationKey::<P>::encoded_len());
    dk.append(&mut dk_pke);
    dk.extend_from_slice(&ek);
    dk.extend_from_slice(&h);
    dk.extend_from_slice(z);
    (
        EncapsulationKey {
            bytes: ek,
            _params: PhantomData,
        },
        DecapsulationKey {
            bytes: dk,
            _params: PhantomData,
        },
    )
}

/// ML-KEM.Encaps from the 32-byte message `m`. This is the deterministic core;
/// [`encapsulate`] draws `m` from an [`Rng`].
pub fn encapsulate_from_seed<P: Parameters>(
    ek: &EncapsulationKey<P>,
    m: &[u8; 32],
) -> (Ciphertext<P>, SharedSecret) {
    let h = sha3_256(&[ek.as_bytes()]);
    let g = sha3_512(&[m, &h]);
    let key = seed_from(&g[..32]);
    let r = seed_from(&g[32..]);
    let c = k_pke_encrypt(ek.as_bytes(), m, &r, P::K, P::ETA1, P::DU, P::DV);
    (
        Ciphertext {
            bytes: c,
            _params: PhantomData,
        },
        SharedSecret(key),
    )
}

/// ML-KEM.Decaps. A ciphertext that does not decrypt correctly gives a
/// pseudo-random secret, so this never fails on a well-formed ciphertext.
pub fn decapsulate<P: Parameters>(dk: &DecapsulationKey<P>, ct: &Ciphertext<P>) -> SharedSecret {
    let k = P::K;
    let bytes = dk.as_bytes();
    let dk_pke = &bytes[..384 * k];
    let ek = &bytes[384 * k..768 * k + 32];
    let h = &bytes[768 * k + 32..768 * k + 64];
    let z = &bytes[768 * k + 64..768 * k + 96];
    let c = ct.as_bytes();

    let m = k_pke_decrypt(dk_pke, c, k, P::DU, P::DV);
    let g = sha3_512(&[&m, h]);
    let key = seed_from(&g[..32]);
    let r = seed_from(&g[32..]);

    let mut implicit = [0u8; SHARED_SECRET_LEN];
    shake256(&[z, c], &mut implicit);

    let c2 = k_pke_encrypt(ek, &m, &r, k, P::ETA1, P::DU, P::DV);
    let mask = equal_mask(c, &c2);
    let mut out = [0u8; SHARED_SECRET_LEN];
    for i in 0..SHARED_SECRET_LEN {
        out[i] = (key[i] & mask) | (implicit[i] & !mask);
    }
    wipe(&mut implicit);
    SharedSecret(out)
}

/// ML-KEM.KeyGen with seeds drawn from `rng`.
pub fn keygen<P: Parameters, R: Rng>(rng: &mut R) -> (EncapsulationKey<P>, DecapsulationKey<P>) {
    let mut d = [0u8; 32];
    let mut z = [0u8; 32];
    rng.fill_bytes(&mut d);
    rng.fill_bytes(&mut z);
    let keys = keygen_from_seed(&d, &z);
    wipe(&mut d);
    wipe(&mut z);
    keys
}

/// ML-KEM.Encaps with the message drawn from `rng`.
pub fn encapsulate<P: Parameters, R: Rng>(
    ek: &EncapsulationKey<P>,
    rng: &mut R,
) -> (Ciphertext<P>, SharedSecret) {
    let mut m = [0u8; 32];
    rng.fill_bytes(&mut m);
    let out = encapsulate_from_seed(ek, &m);
    wipe(&mut m);
    out
}

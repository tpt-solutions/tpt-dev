//! Timing measurements for tpt-dev-pqc: a dudect-style leakage check, and a benchmark.
//!
//! The leakage check times an operation on two classes of input and compares
//! the means with Welch's t-test. As a rule of thumb, |t| above 4.5 suggests
//! that timing depends on the input class. Host timings are noisy, so treat a
//! single run as evidence, not proof, and run it on an idle machine.
//!
//! Run with `cargo run --release -p tpt-dev-pqc --example timing`.

#![allow(clippy::unwrap_used)]

use std::hint::black_box;
use std::time::Instant;

use tpt_dev_pqc::{
    Ciphertext, DsaParameters, MlDsa44, MlDsa65, MlDsa87, MlKem512, MlKem768, MlKem1024,
    Parameters, Rng, decapsulate, dsa_keygen, encapsulate, keygen, sign_deterministic, verify,
};

const SAMPLES: usize = 20_000;

/// A deterministic generator, for repeatable inputs. The timings do not depend on it.
struct Lcg(u64);

impl Rng for Lcg {
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for b in dest {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *b = (self.0 >> 56) as u8;
        }
    }
}

fn mean_var(x: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    let mean = x.iter().sum::<f64>() / n;
    let var = x.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, var)
}

/// Welch's t-statistic for two samples with unequal variances.
fn welch_t(a: &[f64], b: &[f64]) -> f64 {
    let (ma, va) = mean_var(a);
    let (mb, vb) = mean_var(b);
    (ma - mb) / (va / a.len() as f64 + vb / b.len() as f64).sqrt()
}

/// Times decapsulation of a valid ciphertext against an invalid one. Implicit
/// rejection should make both take the same time.
fn decaps_leak<P: Parameters>() -> f64 {
    let mut rng = Lcg(1);
    let (ek, dk) = keygen::<P, _>(&mut rng);
    let (ct, _) = encapsulate(&ek, &mut rng);
    let mut bad_bytes = ct.as_bytes().to_vec();
    bad_bytes[0] ^= 1;
    let bad = Ciphertext::<P>::from_bytes(&bad_bytes).unwrap();

    let mut valid = Vec::with_capacity(SAMPLES);
    let mut invalid = Vec::with_capacity(SAMPLES);
    for i in 0..2 * SAMPLES {
        let (class, out) = if i % 2 == 0 {
            (&ct, &mut valid)
        } else {
            (&bad, &mut invalid)
        };
        let start = Instant::now();
        let secret = decapsulate(&dk, class);
        out.push(start.elapsed().as_nanos() as f64);
        black_box(&secret);
    }
    welch_t(&valid, &invalid)
}

/// Times signing under two different secret keys. Rejection sampling makes the
/// number of attempts depend on the key, so some difference is expected here.
fn sign_leak<P: DsaParameters>() -> f64 {
    let mut rng = Lcg(2);
    let (_, sk_a) = dsa_keygen::<P, _>(&mut rng);
    let (_, sk_b) = dsa_keygen::<P, _>(&mut rng);
    let msg = b"timing message";

    let mut a = Vec::with_capacity(SAMPLES / 4);
    let mut b = Vec::with_capacity(SAMPLES / 4);
    for i in 0..2 * (SAMPLES / 4) {
        let (key, out) = if i % 2 == 0 {
            (&sk_a, &mut a)
        } else {
            (&sk_b, &mut b)
        };
        let start = Instant::now();
        let sig = sign_deterministic(key, msg, b"");
        out.push(start.elapsed().as_nanos() as f64);
        black_box(&sig);
    }
    welch_t(&a, &b)
}

fn time_per_op(iters: u32, mut op: impl FnMut()) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        op();
    }
    start.elapsed().as_nanos() as f64 / f64::from(iters) / 1000.0
}

fn bench_kem<P: Parameters>() {
    let mut rng = Lcg(3);
    let keys = keygen::<P, _>(&mut rng);
    let (ek, dk) = keys;
    let (ct, _) = encapsulate(&ek, &mut rng);
    println!(
        "{:12} keygen {:8.1} us  encaps {:8.1} us  decaps {:8.1} us",
        P::NAME,
        time_per_op(200, || {
            black_box(keygen::<P, _>(&mut rng));
        }),
        time_per_op(500, || {
            black_box(encapsulate(&ek, &mut rng));
        }),
        time_per_op(500, || {
            black_box(decapsulate(&dk, &ct));
        }),
    );
}

fn bench_dsa<P: DsaParameters>() {
    let mut rng = Lcg(4);
    let (vk, sk) = dsa_keygen::<P, _>(&mut rng);
    let sig = sign_deterministic(&sk, b"bench", b"").unwrap();
    println!(
        "{:12} keygen {:8.1} us  sign {:8.1} us  verify {:8.1} us",
        P::NAME,
        time_per_op(100, || {
            black_box(dsa_keygen::<P, _>(&mut rng));
        }),
        time_per_op(100, || {
            let _ = black_box(sign_deterministic(&sk, b"bench", b""));
        }),
        time_per_op(200, || {
            black_box(verify(&vk, b"bench", b"", &sig));
        }),
    );
}

fn main() {
    println!("Welch t-statistics over {SAMPLES} samples per class (|t| > 4.5 suggests leakage)");
    println!(
        "ML-KEM decapsulation, valid vs invalid ciphertext: 512 {:+.2}, 768 {:+.2}, 1024 {:+.2}",
        decaps_leak::<MlKem512>(),
        decaps_leak::<MlKem768>(),
        decaps_leak::<MlKem1024>(),
    );
    println!(
        "ML-DSA signing, key A vs key B (expected to differ, see README): 44 {:+.2}, 65 {:+.2}, 87 {:+.2}",
        sign_leak::<MlDsa44>(),
        sign_leak::<MlDsa65>(),
        sign_leak::<MlDsa87>(),
    );
    println!();
    println!("Mean time per operation on this host (release build):");
    bench_kem::<MlKem512>();
    bench_kem::<MlKem768>();
    bench_kem::<MlKem1024>();
    bench_dsa::<MlDsa44>();
    bench_dsa::<MlDsa65>();
    bench_dsa::<MlDsa87>();
}

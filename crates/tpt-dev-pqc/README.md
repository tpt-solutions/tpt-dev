# tpt-dev-pqc

Pure-Rust post-quantum cryptography: ML-KEM and ML-DSA.

> **Not independently audited.** This code has been checked against the NIST known-answer vectors and against its own tests. No security firm has reviewed it, and it has not been tested against side-channel attacks on hardware. Do not rely on it to protect real data until it has been audited.

## Overview

`tpt-dev-pqc` implements **ML-KEM (FIPS 203)**, a key-encapsulation mechanism, and **ML-DSA (FIPS 204)**, a digital signature scheme, each at all three standard parameter sets. It is written in safe Rust with no dependencies, and is `#![no_std]` with `alloc`, so it can run on embedded targets as well as on a host.

It is for projects in the `tpt-dev` workspace and others that need post-quantum key exchange or signatures and want a small implementation they can read. It has no dependencies on other `tpt-dev-*` crates. Read the limits below before using it: timing has been measured on a host only, and the crate has had no outside review.

## Features

- **ML-KEM parameter sets:** `MlKem512`, `MlKem768` and `MlKem1024`, through the `Parameters` trait.
- **ML-KEM operations:** `keygen` and `encapsulate` take an `Rng` you supply. `decapsulate` takes no randomness. The `*_from_seed` functions are deterministic, for testing and for the known-answer vectors.
- **Checked keys and ciphertexts:** `EncapsulationKey`, `DecapsulationKey` and `Ciphertext` check their length on parsing. An encapsulation key is also checked for the modulus condition in FIPS 203.
- **Implicit rejection:** decapsulating a bad ciphertext gives a pseudo-random secret, not an error. The secret is chosen with a mask, so no branch depends on whether the ciphertext was valid.
- **ML-DSA parameter sets:** `MlDsa44`, `MlDsa65` and `MlDsa87`, through the `DsaParameters` trait.
- **ML-DSA operations:** `sign_deterministic` gives the same signature for the same key and message. `dsa_sign` takes an `Rng` and draws a fresh randomizer, so each signature differs. `verify` checks a signature against its message and context string. Contexts longer than 255 bytes are refused.
- **Pure mode only:** the message is wrapped with its context as FIPS 204 specifies. The pre-hashed (HashML-DSA) variants are not implemented.

## Installation

```sh
cargo add tpt-dev-pqc
```

| Feature | Enables | Default |
| --- | --- | --- |
| `std` | Links `std`. It adds no public items today. | off |

The crate is `#![no_std]` and needs `alloc`. Keys, signatures and ciphertexts are `Vec<u8>`-backed. CI builds it for `thumbv7em-none-eabihf`.

## Quick start

### ML-KEM

```rust
use tpt_dev_pqc::{MlKem768, Rng, decapsulate, encapsulate, keygen};

// A deterministic generator, for the example only. Use a CSPRNG in real code.
struct Counter(u8);
impl Rng for Counter {
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for b in dest {
            self.0 = self.0.wrapping_add(0x9d);
            *b = self.0;
        }
    }
}

let mut rng = Counter(1);
let (ek, dk) = keygen::<MlKem768, _>(&mut rng);
let (ct, sent) = encapsulate(&ek, &mut rng);
let received = decapsulate(&dk, &ct);
assert_eq!(sent.as_bytes(), received.as_bytes());
```

### ML-DSA

```rust
use tpt_dev_pqc::{MlDsa65, Rng, dsa_keygen, sign_deterministic, verify};

// A deterministic generator, for the example only. Use a CSPRNG in real code.
struct Counter(u8);
impl Rng for Counter {
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for b in dest {
            self.0 = self.0.wrapping_add(0x9d);
            *b = self.0;
        }
    }
}

let mut rng = Counter(7);
let (vk, sk) = dsa_keygen::<MlDsa65, _>(&mut rng);

let sig = sign_deterministic(&sk, b"message", b"context").unwrap();
assert!(verify(&vk, b"message", b"context", &sig));
assert!(!verify(&vk, b"other message", b"context", &sig));
assert!(!verify(&vk, b"message", b"other context", &sig));
```

## More examples

- **`timing`:** a dudect-style leakage check (Welch's t-test, |t| above 4.5 suggests leakage) and a host benchmark of ML-KEM and ML-DSA.

  ```sh
  cargo run --release -p tpt-dev-pqc --example timing
  ```

## Design notes and guarantees

- **Implicit rejection** in ML-KEM decapsulation uses masks, not branches, to choose between the real and the pseudo-random secret.
- **Fixed-work signing.** ML-DSA signing makes a fixed budget of 64 attempts for every signature and keeps the first one that passes its checks, selecting it with masks, so the number of attempts does not show in the timing. The output is the same as the standard retry loop. The loop continues past 64 only when no attempt passes, which is rare (about 1e-6 or less per signature), and that path is not constant time.
- **Branch-free reductions.** Reductions and comparisons in the signing path use masks, not `if`.
- **Zeroisation, best effort.** Secret buffers are overwritten on drop, using `core::hint::black_box` to stop the compiler discarding the writes. Copies made by the caller or by the optimiser are not covered.
- **Randomness is injected.** The `Rng` trait is the only source of entropy. The crate has none of its own and is not tied to any RNG crate.
- **No unsafe code,** under the workspace `unsafe_code` lint.

### Verification

- **NIST known-answer vectors.** The first three records of each parameter set are in `tests/fixtures`, for both schemes. The KAT tests check every value: keys, ciphertext, shared secret, signature, and the implicit-rejection secret.
  - **ML-KEM:** all 3,000 records of the full NIST files pass (1,000 per parameter set, from the `post-quantum-cryptography/KAT` repository).
  - **ML-DSA:** all 300 records of the deterministic, pure-mode NIST files pass (100 per parameter set). The hedged and hashed variants are not run.
  - Run the full files with `TPT_KAT_DIR=<dir> cargo test -p tpt-dev-pqc --release --test kat_mlkem -- --ignored`, and likewise `--test kat_mldsa`. `<dir>` holds the `.rsp` files.
- **Primitives.** The Keccak sponge matches SHA3-256, SHA3-512 and SHAKE128/256 reference digests. The number-theoretic transforms match a schoolbook negacyclic product, and ML-DSA's transform round-trips.
- **API tests.** `tests/kem_api.rs` and `tests/dsa_api.rs` check the standard sizes, length and modulus refusals, round trips, tampered ciphertexts and signatures, and the context-length limit.

## Limitations and scope

- **Not audited, and not side-channel tested on hardware.** See the notice at the top.
- **Randomness is the caller's job.** A weak `Rng` breaks both schemes.
- **Timing, measured on the host only.** The `timing` example gave these results on one machine, in a dudect-style check:
  - **ML-KEM decapsulation, valid vs invalid ciphertext:** |t| of 1.84 or less at all three levels in an earlier run, and 1.17 or less in the latest run. No leak was detected. Implicit rejection does the same work either way.
  - **ML-DSA signing, one key vs another:** before the fixed-budget change, |t| was about 40 to 56 for ML-DSA-44 and -65, and about 1.5 for ML-DSA-87. After it, in the latest run, |t| is 0.20 for ML-DSA-44, -0.13 for -65 and 0.43 for -87, all well below 4.5. This is a host measurement of 20,000 samples per class, not evidence of constant time on other hardware.
  - **Remaining data-dependent timing:** the sampling inside each signing attempt (SampleInBall) still has data-dependent loops, and key generation still rejection-samples its secret seed.
  - **Division on Cortex-M, done:** the NTT, inverse NTT and pointwise products use Montgomery reduction (R = 2^32), which needs only 32-bit multiplies. On `thumbv7em-none-eabihf` the library no longer calls `__aeabi_ldivmod` (it was 16 calls). The outputs are bit-identical to the previous `i64 % q` code.
  - Host timings vary between runs.
- **Benchmarks (host, release, one run):** ML-KEM-512/768/1024 decapsulation 60, 88 and 128 µs. ML-DSA-44/65/87 signing 14.4, 20.5 and 27.7 ms (about 9 to 15 times slower than before the fixed budget, which was 1.6, 1.6 and 1.8 ms), and verification 0.22, 0.31 and 0.48 ms. Cortex-M numbers are not measured.
- **Signing is slower by design.** The fixed 64-attempt budget is the cost of hiding the retry count.
- **No HashML-DSA,** and the hedged ML-DSA variant is not checked against NIST vectors.
- **Heap use.** Internal vectors of polynomials use `alloc`, so the crate needs an allocator.

## Related crates

`tpt-dev-pqc` does not depend on, and is not depended on by, any other workspace crate. [`tpt-dev-zk`](https://docs.rs/tpt-dev-zk) and [`tpt-dev-tee`](https://docs.rs/tpt-dev-tee) are other security-related crates in the workspace.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

# tpt-dev-pqc

Pure-Rust post-quantum cryptography. **ML-KEM (FIPS 203)** and **ML-DSA (FIPS 204)** are implemented for all three security levels each.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

> **Not independently audited.** This code has been checked against the NIST known-answer vectors and against its own tests. No security firm has reviewed it, and it has not been tested against side-channel attacks on hardware. Do not rely on it to protect real data until it has been audited.

## ML-KEM (key encapsulation)

- **Parameter sets:** `MlKem512`, `MlKem768` and `MlKem1024`, through the `Parameters` trait.
- **Operations:** `keygen` and `encapsulate` take an `Rng` you supply. `decapsulate` takes no randomness. The `*_from_seed` functions are deterministic, for testing and for the known-answer vectors.
- **Keys and ciphertexts:** `EncapsulationKey`, `DecapsulationKey` and `Ciphertext` check their length on parsing. An encapsulation key is also checked for the modulus condition in FIPS 203.
- **Implicit rejection:** decapsulating a bad ciphertext gives a pseudo-random secret, not an error. The secret is chosen with a mask, so no branch depends on whether the ciphertext was valid.

## ML-DSA (signatures)

- **Parameter sets:** `MlDsa44`, `MlDsa65` and `MlDsa87`, through the `DsaParameters` trait.
- **Operations:** `sign_deterministic` gives the same signature for the same key and message. `sign` takes an `Rng` and draws a fresh randomizer, so each signature differs. `verify` checks a signature against its message and context string. Contexts longer than 255 bytes are refused.
- **Pure mode:** the message is wrapped with its context as FIPS 204 specifies. The crate does not implement the pre-hashed (HashML-DSA) variants.
- **Timing of signing.** Signing makes a fixed budget of 64 attempts for every signature, and keeps the first that passes its checks with masks, so the number of attempts no longer shows in the timing. The output is the same as the standard retry loop. The loop continues past 64 only when no attempt passes, which is rare (about 1e-6 or less per signature), and that path is not constant time. The sampling inside each attempt (SampleInBall) still has data-dependent loops, and key generation still rejection-samples its secret seed.

## Verification

- **NIST known-answer vectors.** The first three records of each parameter set are in `tests/fixtures`, for both schemes. The KAT tests check every value: keys, ciphertext, shared secret, signature, and the implicit-rejection secret.
  - **ML-KEM.** All 3,000 records of the full NIST files pass (1,000 per parameter set, from the `post-quantum-cryptography/KAT` repository).
  - **ML-DSA.** All 300 records of the deterministic, pure-mode NIST files pass (100 per parameter set). The hedged and hashed variants are not run.
  - Run the full files with `TPT_KAT_DIR=<dir> cargo test -p tpt-dev-pqc --release --test kat_mlkem -- --ignored`, and likewise `--test kat_mldsa`. `<dir>` holds the `.rsp` files.
- **Primitives.** The Keccak sponge matches SHA3-256, SHA3-512 and SHAKE128/256 reference digests. The number-theoretic transforms match a schoolbook negacyclic product, and ML-DSA's transform round-trips.
- **API tests.** `tests/kem_api.rs` and `tests/dsa_api.rs` check the standard sizes, length and modulus refusals, round trips, tampered ciphertexts and signatures, and the context-length limit.

### Example: ML-KEM

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

## Limits and review notes

- **Randomness is the caller's job.** The crate has no entropy source. A weak `Rng` breaks both schemes.
- **Timing, measured on the host only.** `cargo run --release -p tpt-dev-pqc --example timing` runs a dudect-style check (Welch's t-test, |t| above 4.5 suggests leakage) and prints benchmarks. A run on this machine gave:
  - **ML-KEM decapsulation, valid vs invalid ciphertext:** |t| of 1.84 or less at all three levels in an earlier run, and 1.17 or less in the latest run. No leak was detected. Implicit rejection does the same work either way.
  - **ML-DSA signing, one key vs another:** before the fixed-budget change, |t| was about 40 to 56 for ML-DSA-44 and -65, and about 1.5 for ML-DSA-87. After it, in the latest run, |t| is 0.20 for ML-DSA-44, -0.13 for -65 and 0.43 for -87, all well below 4.5. This is a host measurement of 20,000 samples per class, not evidence of constant time on other hardware.
  - **Reductions and comparisons** in the signing path are branch-free (masks, not `if`). The NTT and pointwise products use `i64 % q`. On `thumbv7em-none-eabihf` that compiles to 16 calls to the `__aeabi_ldivmod` library routine, whose timing is not checked. That is open.
  - Host timings vary between runs.
- **Benchmarks (host, release, one run):** ML-KEM-512/768/1024 decapsulation 60, 88 and 128 µs. ML-DSA-44/65/87 signing 14.4, 20.5 and 27.7 ms (about 9 to 15 times slower than before the fixed budget, which was 1.6, 1.6 and 1.8 ms), and verification 0.22, 0.31 and 0.48 ms. Cortex-M numbers are not measured.
- **Zeroisation is best effort.** Secret buffers are overwritten on drop, using `core::hint::black_box` to stop the compiler discarding the writes. The crate cannot guarantee that copies made by the caller or by the optimiser are gone.
- **Heap use.** Internal vectors of polynomials use `alloc`. Keys, signatures and ciphertexts are `Vec<u8>`, so the crate needs an allocator.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

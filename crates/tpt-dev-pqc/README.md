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
- **Timing of signing.** Signing draws masking vectors and retries until its checks pass. The number of retries depends on the key and the message. That is part of the scheme, but it means signing time can leak information about the key to an observer who can time many signatures.

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
  - **ML-KEM decapsulation, valid vs invalid ciphertext:** |t| of 1.84 or less at all three levels, so no leak was detected. Implicit rejection does the same work either way.
  - **ML-DSA signing, one key vs another:** |t| of about 40 to 56 for ML-DSA-44 and -65, and about 1.5 for ML-DSA-87. **This is a real leak.** Signing time depends on the secret key, most likely through the number of rejection-sampling attempts. The README's earlier warning about this is confirmed. Fixing it needs a different signing loop, such as a fixed number of attempts with dummy work, and that is not done.
  - The field reduction uses `%` by a constant. Compilers turn that into multiplication on common targets, but the device has not been checked.
  - Host timings vary between runs. The ML-DSA figures moved between about 40 and 56 in repeated runs, though all were large.
- **Benchmarks (host, release, one run):** ML-KEM-512/768/1024 decapsulation 69, 120 and 156 µs. ML-DSA-44/65/87 signing 1.6, 1.6 and 1.8 ms, and verification 0.21, 0.33 and 0.52 ms. Cortex-M numbers are not measured.
- **Zeroisation is best effort.** Secret buffers are overwritten on drop, using `core::hint::black_box` to stop the compiler discarding the writes. The crate cannot guarantee that copies made by the caller or by the optimiser are gone.
- **Heap use.** Internal vectors of polynomials use `alloc`. Keys, signatures and ciphertexts are `Vec<u8>`, so the crate needs an allocator.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

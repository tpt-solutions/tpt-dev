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
- **Constant time is argued, not measured.** The code avoids branches on secret data, and the ML-KEM implicit-rejection step uses masks. The field reduction uses `%` by a constant. Compilers turn that into multiplication on common targets, but this has not been checked on the device. A dudect-style timing test is still to be written. ML-DSA's rejection loop is described above.
- **Zeroisation is best effort.** Secret buffers are overwritten on drop, using `core::hint::black_box` to stop the compiler discarding the writes. The crate cannot guarantee that copies made by the caller or by the optimiser are gone.
- **Heap use.** Internal vectors of polynomials use `alloc`. Keys, signatures and ciphertexts are `Vec<u8>`, so the crate needs an allocator.
- **No benchmarks yet.** Performance on the host and on Cortex-M is still to be measured.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

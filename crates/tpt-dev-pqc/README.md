# tpt-dev-pqc

Pure-Rust post-quantum cryptography. **ML-KEM (FIPS 203)** is implemented for all three parameter sets. **ML-DSA (FIPS 204)** is not implemented yet.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

> **Not independently audited.** This code has been checked against the NIST known-answer vectors and against its own tests. No security firm has reviewed it, and it has not been tested against side-channel attacks on hardware. Do not rely on it to protect real data until it has been audited.

## ML-KEM

- **Parameter sets:** `MlKem512`, `MlKem768` and `MlKem1024`, through the `Parameters` trait.
- **Operations:** `keygen` and `encapsulate` take an `Rng` you supply. `decapsulate` takes no randomness. The `*_from_seed` functions are deterministic, for testing and for the known-answer vectors.
- **Keys and ciphertexts:** `EncapsulationKey`, `DecapsulationKey` and `Ciphertext` check their length on parsing. An encapsulation key is also checked for the modulus condition in FIPS 203.
- **Implicit rejection:** decapsulating a bad ciphertext gives a pseudo-random secret, not an error. The secret is chosen with a mask, so no branch depends on whether the ciphertext was valid.

### Verification

- **NIST known-answer vectors.** The first three records of each parameter set are in `tests/fixtures`, and `tests/kat_mlkem.rs` checks every value: keys, ciphertext, shared secret, and the implicit-rejection secret. The full NIST files (1,000 records per parameter set, from the `post-quantum-cryptography/KAT` repository) also pass. Run them with `TPT_KAT_DIR=<dir> cargo test -p tpt-dev-pqc --release --test kat_mlkem -- --ignored`.
- **Primitives.** The Keccak sponge matches SHA3-256, SHA3-512 and SHAKE128/256 reference digests. The number-theoretic transform matches a schoolbook negacyclic product.
- **API tests.** `tests/kem_api.rs` checks the standard sizes, length and modulus refusals, round trips at every level, and the effect of a tampered ciphertext.

### Example

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

- **Randomness is the caller's job.** The crate has no entropy source. A weak `Rng` breaks the scheme.
- **Constant time is argued, not measured.** The code avoids branches on secret data, and the implicit-rejection step uses masks. The field reduction uses `%` by a constant. Compilers turn that into multiplication on common targets, but this has not been checked on the device. A dudect-style timing test is still to be written.
- **Zeroisation is best effort.** Secret buffers are overwritten on drop, using `core::hint::black_box` to stop the compiler discarding the writes. The crate cannot guarantee that copies made by the caller or by the optimiser are gone.
- **Heap use.** The internal vectors of polynomials use `alloc`. Keys and ciphertexts are `Vec<u8>`, so the crate needs an allocator.
- **No benchmarks yet.** Performance on the host and on Cortex-M is still to be measured.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

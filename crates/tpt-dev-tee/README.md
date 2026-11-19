# tpt-dev-tee

A Trusted Execution Environment abstraction. Code written against three traits runs on any backend that implements them. The traits cover secure memory, attestation, and calls into secure services.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

- **Traits:** `SecureMemory` (memory only the secure world may touch), `Attestation` (a report, bound to a verifier's nonce, of what the secure world runs), and `SecureCall` (calls from the normal world into secure services).
- **Mock backend:** `mock::MockTee` enforces the same rules in host tests. Its attestation tag is **not** a cryptographic MAC. It only lets tests check that verification rejects changed inputs.
- **TrustZone model:** `trustzone` models the Cortex-M secure and non-secure partition, with deny-by-default for uncovered addresses, and a gate that enters the secure world only at registered entry points and does not nest.

## Example

```rust
use tpt_dev_tee::mock::{MockTee, measure};
use tpt_dev_tee::{Attestation, SecureMemory, TeeError, World};

let mut tee: MockTee<64, 2> = MockTee::new([1; 32], measure(b"firmware"));
assert_eq!(tee.write(World::Normal, 0, &[1]), Err(TeeError::Denied));
tee.write(World::Secure, 0, &[1]).unwrap();

let nonce = [2u8; 32];
let report = tee.quote(&nonce).unwrap();
assert!(tee.verify(&report, &nonce));
```

## Not done yet

- **TrustZone hardware backend.** The partition and gate are modelled and tested, but the crate does not program the SAU, the IDAU, or the secure gateway veneers. Those are device-specific, and they are not written or tested on hardware.
- **Intel TDX backend.** It needs Intel's guest interface, and it cannot be built or tested in this environment.

## Safety review

This crate is a set of rules and a mock. It is not a security boundary by itself. Points to check before a hardware backend relies on it:

- **Mock attestation is not secure.** The tag is a fixed mixing function written for tests. Anyone can forge a report with it. A real backend must use a standard MAC or signature under a key held only by the secure world.
- **Verification compares tags in constant time** (`constant_time_eq`). The comparison does not stop at the first differing byte. The mock's digest is not designed to resist attack, so the constant-time check protects nothing in the mock; a real backend needs its own review.
- **Verification checks the tag, not the measurement.** A verifier that skips comparing `Report::measurement` to its expected value accepts any secure image. The doc comment on `Attestation::verify` states this.
- **Partition defaults to secure.** An address no region covers is secure, so a missing region fails closed, not open.
- **Entry is by registered address only.** `Gate::enter` refuses any other address and refuses to nest. The gate models the rule, but the hardware veneers that enforce it on a device are not written here.
- **Range checks cover the whole access.** An access that crosses from non-secure into secure memory is refused, because every byte must be permitted.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

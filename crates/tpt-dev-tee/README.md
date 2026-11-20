# tpt-dev-tee

Trusted Execution Environment abstraction for TrustZone and Intel TDX.

> **Not independently audited.** This crate has not had an independent security review. It is a set of traits, a test mock and a policy model. It is not a security boundary by itself, and the mock backend must not be used to protect anything real. See [Limitations and scope](#limitations-and-scope).

## Overview

A Trusted Execution Environment (TEE) splits a device into a trusted secure world and an untrusted normal world. `tpt-dev-tee` defines three traits that code can be written against: secure memory, attestation, and calls into secure services. A backend implements the traits for one isolation technology.

The crate ships two things that implement or model the rules. `mock::MockTee` is a host-test backend that enforces the same access rules as a hardware backend. `trustzone` models the Cortex-M secure and non-secure partition and the gate that enters the secure world, so the policy can be tested on the host.

It does **not** include a hardware backend. The TrustZone model does not program any hardware, and there is no Intel TDX backend (see below). The crate is `no_std`, allocates nothing and has no dependencies. It stands alone in the workspace.

## Features

- **Traits:** `SecureMemory` (memory only the secure world may touch), `Attestation` (a report, bound to a verifier's nonce, of what the secure world runs) and `SecureCall` (calls from the normal world into secure services).
- **Mock backend:** `mock::MockTee<MEM, SERVICES>` has `MEM` bytes of secure memory and a table of `SERVICES` services. The normal world is denied every memory access, and services run only through `SecureCall::call`.
- **TrustZone partition model:** `trustzone::Partition` holds non-overlapping address regions marked `Secure` or `NonSecure`. An address no region covers is secure.
- **TrustZone gate model:** `trustzone::Gate` enters the secure world only at registered entry points, and does not nest.
- **Error type:** `TeeError` has `Denied`, `OutOfRange`, `UnknownService`, `Full`, `BufferTooSmall` and `Refused`.

## Installation

```sh
cargo add tpt-dev-tee
```

The crate is `#![no_std]` and does not need `alloc`. It has no dependencies.

| Feature | Enables | Default |
|---|---|---|
| `std` | Links `std`. It adds no public API. | off |

## Quick start

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

The TrustZone model is used like this:

```rust
use tpt_dev_tee::trustzone::{Gate, Partition, Region, Security};
use tpt_dev_tee::{TeeError, World};

let regions = [
    Region { start: 0x0000, end: 0x1000, security: Security::NonSecure },
    Region { start: 0x1000, end: 0x2000, security: Security::Secure },
];
let partition = Partition::new(&regions).unwrap();
let entries = [0x0100];
let mut gate = Gate::new(&entries);

assert_eq!(gate.check(&partition, 0x1000, 4), Err(TeeError::Denied));
gate.enter(0x0100).unwrap();
assert_eq!(gate.world(), World::Secure);
assert_eq!(gate.check(&partition, 0x1000, 4), Ok(()));
gate.leave().unwrap();
```

## Design notes and guarantees

- **Normal world is denied.** `MockTee` refuses every read and write from `World::Normal` with `TeeError::Denied`. Out-of-bounds secure accesses return `OutOfRange`, and the bounds check uses `checked_add`, so it cannot overflow.
- **Partition defaults to secure.** An address no region covers is secure, so a missing region fails closed. `Partition::new` returns `None` for an empty or overlapping region.
- **Range checks cover the whole access.** `Partition::allows` requires every byte of an access to be permitted, so an access that crosses from non-secure into secure memory is refused.
- **Entry is by registered address only.** `Gate::enter` refuses any other address and refuses to enter when already in the secure world. `Gate::leave` is refused from the normal world.
- **Verification compares tags without an early exit.** `MockTee::verify` compares the tag with a loop that does not stop at the first differing byte. The nonce is compared first and with `!=`, since it is not secret.
- **Fixed memory.** The mock uses const generics, so it has fixed memory and a fixed service table. Registering an existing service id replaces it and needs no slot.

## Limitations and scope

This crate is a set of rules and a mock. It is not a security boundary by itself. Points to check before a hardware backend relies on it:

- **Mock attestation is not secure.** The tag is a fixed mixing function written for tests. It is not a cryptographic MAC, and anyone can forge a report with it. A real backend must use a standard MAC or signature under a key held only by the secure world.
- **The mock's digest is not designed to resist attack.** The tag comparison does not exit early, but that protects nothing in the mock. Nothing in the code guarantees constant-time execution after compilation. A real backend needs its own review.
- **Verification checks the tag, not the measurement.** A verifier that skips comparing `Report::measurement` to its expected value accepts any secure image. The doc comment on `Attestation::verify` says so.
- **No TrustZone hardware backend.** The partition and gate are modelled and tested on the host. The crate does not program the security attribution unit (SAU), the implementation-defined attribution unit (IDAU) or the secure gateway veneers. Those are device-specific, and they are not written or tested on hardware. The gate models the entry rule, but nothing here enforces it on a device.
- **No Intel TDX backend.** It needs Intel's guest interface, and it cannot be built or tested in this environment.
- **Not a model of side channels.** Nothing here addresses timing, cache or fault-injection attacks.

## Related crates

This crate has no dependencies on other `tpt-dev-*` crates, and no workspace crate depends on it. See the [tpt-dev workspace](https://github.com/tpt-solutions/tpt-dev) for the others.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

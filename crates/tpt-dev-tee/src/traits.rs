//! The TEE trait set: secure memory, attestation, and secure calls.
//!
//! A backend implements these for one isolation technology. Code written
//! against the traits runs unchanged on the mock backend in host tests and on a
//! hardware backend on the device.

use core::fmt;

/// Which side of the isolation boundary is acting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum World {
    /// The untrusted side: the rich OS, or the application.
    Normal,
    /// The trusted side.
    Secure,
}

/// Why a TEE operation was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeeError {
    /// The caller's world may not access the memory.
    Denied,
    /// The access falls outside the memory.
    OutOfRange,
    /// No service is registered under the id.
    UnknownService,
    /// A table has no free slot.
    Full,
    /// The output buffer is too small for the reply.
    BufferTooSmall,
    /// The service refused the request.
    Refused,
}

impl fmt::Display for TeeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TeeError::Denied => "access denied by the isolation boundary",
            TeeError::OutOfRange => "access outside the memory",
            TeeError::UnknownService => "no secure service has that id",
            TeeError::Full => "no free slot in the table",
            TeeError::BufferTooSmall => "output buffer too small",
            TeeError::Refused => "the secure service refused the request",
        })
    }
}

/// Memory that only the secure world may touch.
pub trait SecureMemory {
    /// Size of the memory, in bytes.
    fn capacity(&self) -> usize;

    /// Reads `out.len()` bytes at `offset`, if `world` may access them.
    fn read(&self, world: World, offset: usize, out: &mut [u8]) -> Result<(), TeeError>;

    /// Writes `data` at `offset`, if `world` may access it.
    fn write(&mut self, world: World, offset: usize, data: &[u8]) -> Result<(), TeeError>;
}

/// Length of a digest, nonce or tag, in bytes.
pub const DIGEST_LEN: usize = 32;

/// A digest of the code loaded into the secure world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement(pub [u8; DIGEST_LEN]);

/// Evidence, from the secure world, of what it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    /// The measurement of the secure code.
    pub measurement: Measurement,
    /// The nonce the verifier supplied, so the report is not replayed.
    pub nonce: [u8; DIGEST_LEN],
    /// Authentication tag over the measurement and nonce.
    pub tag: [u8; DIGEST_LEN],
}

/// Attestation: the secure world proves what it is running.
pub trait Attestation {
    /// Produces a report bound to `nonce`.
    fn quote(&self, nonce: &[u8; DIGEST_LEN]) -> Result<Report, TeeError>;

    /// Checks that `report` was produced for `nonce` by this trust anchor.
    ///
    /// This checks the tag only. The verifier must still compare
    /// `report.measurement` against the value it expects.
    fn verify(&self, report: &Report, nonce: &[u8; DIGEST_LEN]) -> bool;
}

/// Calls from the normal world into services in the secure world.
pub trait SecureCall {
    /// Calls `service` with `args`. The reply is written to `out`, and its length is returned.
    fn call(&mut self, service: u32, args: &[u8], out: &mut [u8]) -> Result<usize, TeeError>;
}

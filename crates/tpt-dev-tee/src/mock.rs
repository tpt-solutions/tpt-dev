//! A mock TEE for host tests.
//!
//! It enforces the same rules as a hardware backend: the normal world cannot
//! touch secure memory, and services run only through [`SecureCall::call`].
//!
//! The attestation tag is not a cryptographic MAC. It is a fixed mixing
//! function, enough to test that verification rejects a changed nonce,
//! measurement or key. Do not use the mock to protect anything real.

use crate::traits::{
    Attestation, DIGEST_LEN, Measurement, Report, SecureCall, SecureMemory, TeeError, World,
};

/// A secure service: it gets the secure memory, the arguments, and the reply buffer.
pub type Handler<const MEM: usize> =
    fn(&mut [u8; MEM], &[u8], &mut [u8]) -> Result<usize, TeeError>;

/// A mock TEE with `MEM` bytes of secure memory and room for `SERVICES` services.
pub struct MockTee<const MEM: usize, const SERVICES: usize> {
    memory: [u8; MEM],
    key: [u8; DIGEST_LEN],
    measurement: Measurement,
    services: [Option<(u32, Handler<MEM>)>; SERVICES],
}

impl<const MEM: usize, const SERVICES: usize> MockTee<MEM, SERVICES> {
    /// A mock with zeroed secure memory, the given attestation key, and the
    /// measurement it reports for its secure code.
    pub const fn new(key: [u8; DIGEST_LEN], measurement: Measurement) -> Self {
        Self {
            memory: [0; MEM],
            key,
            measurement,
            services: [None; SERVICES],
        }
    }

    /// Registers `handler` under `id`. A second registration for the same id
    /// replaces the first.
    pub fn register(&mut self, id: u32, handler: Handler<MEM>) -> Result<(), TeeError> {
        if let Some(slot) = self
            .services
            .iter_mut()
            .flatten()
            .find(|(existing, _)| *existing == id)
        {
            slot.1 = handler;
            return Ok(());
        }
        let free = self.services.iter_mut().find(|s| s.is_none());
        match free {
            Some(slot) => {
                *slot = Some((id, handler));
                Ok(())
            }
            None => Err(TeeError::Full),
        }
    }
}

impl<const MEM: usize, const SERVICES: usize> SecureMemory for MockTee<MEM, SERVICES> {
    fn capacity(&self) -> usize {
        MEM
    }

    fn read(&self, world: World, offset: usize, out: &mut [u8]) -> Result<(), TeeError> {
        check(world, offset, out.len(), MEM)?;
        out.copy_from_slice(&self.memory[offset..offset + out.len()]);
        Ok(())
    }

    fn write(&mut self, world: World, offset: usize, data: &[u8]) -> Result<(), TeeError> {
        check(world, offset, data.len(), MEM)?;
        self.memory[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }
}

fn check(world: World, offset: usize, len: usize, capacity: usize) -> Result<(), TeeError> {
    if world == World::Normal {
        return Err(TeeError::Denied);
    }
    match offset.checked_add(len) {
        Some(end) if end <= capacity => Ok(()),
        _ => Err(TeeError::OutOfRange),
    }
}

impl<const MEM: usize, const SERVICES: usize> Attestation for MockTee<MEM, SERVICES> {
    fn quote(&self, nonce: &[u8; DIGEST_LEN]) -> Result<Report, TeeError> {
        Ok(Report {
            measurement: self.measurement,
            nonce: *nonce,
            tag: tag(&self.key, nonce, &self.measurement),
        })
    }

    fn verify(&self, report: &Report, nonce: &[u8; DIGEST_LEN]) -> bool {
        if &report.nonce != nonce {
            return false;
        }
        let expected = tag(&self.key, &report.nonce, &report.measurement);
        constant_time_eq(&expected, &report.tag)
    }
}

impl<const MEM: usize, const SERVICES: usize> SecureCall for MockTee<MEM, SERVICES> {
    fn call(&mut self, service: u32, args: &[u8], out: &mut [u8]) -> Result<usize, TeeError> {
        let handler = self
            .services
            .iter()
            .flatten()
            .find(|(id, _)| *id == service)
            .map(|(_, handler)| *handler)
            .ok_or(TeeError::UnknownService)?;
        handler(&mut self.memory, args, out)
    }
}

/// Computes a measurement of a code image, with the mock's digest function.
pub fn measure(image: &[u8]) -> Measurement {
    Measurement(digest([0x6d65_6173_7572_65d1, 0, 0, 0], &[image]))
}

fn tag(
    key: &[u8; DIGEST_LEN],
    nonce: &[u8; DIGEST_LEN],
    measurement: &Measurement,
) -> [u8; DIGEST_LEN] {
    digest(seed(key), &[nonce, &measurement.0])
}

fn seed(key: &[u8; DIGEST_LEN]) -> [u64; 4] {
    let mut lanes = [0u64; 4];
    let (chunks, _) = key.as_chunks::<8>();
    for (lane, chunk) in lanes.iter_mut().zip(chunks) {
        *lane = u64::from_le_bytes(*chunk);
    }
    lanes
}

/// Four 64-bit lanes, mixed byte by byte. Not a cryptographic hash.
fn digest(seed: [u64; 4], parts: &[&[u8]]) -> [u8; DIGEST_LEN] {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = seed;
    for part in parts {
        for &byte in part.iter() {
            for (i, lane) in h.iter_mut().enumerate() {
                *lane ^= u64::from(byte) ^ i as u64;
                *lane = lane.wrapping_mul(PRIME);
            }
            h[1] ^= h[0].rotate_left(13);
            h[2] ^= h[1].rotate_left(29);
            h[3] ^= h[2].rotate_left(7);
            h[0] ^= h[3];
        }
        // Mix in the length, so "ab" and "a" followed by "b" differ.
        for lane in h.iter_mut() {
            *lane ^= part.len() as u64;
            *lane = lane.wrapping_mul(PRIME);
        }
    }
    let mut out = [0u8; DIGEST_LEN];
    let (chunks, _) = out.as_chunks_mut::<8>();
    for (chunk, lane) in chunks.iter_mut().zip(h) {
        *chunk = lane.to_le_bytes();
    }
    out
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const KEY: [u8; DIGEST_LEN] = [7; DIGEST_LEN];
    const NONCE: [u8; DIGEST_LEN] = [9; DIGEST_LEN];

    fn tee() -> MockTee<64, 4> {
        MockTee::new(KEY, measure(b"secure firmware v1"))
    }

    fn sum(mem: &mut [u8; 64], args: &[u8], out: &mut [u8]) -> Result<usize, TeeError> {
        let total: u32 = args.iter().map(|&b| u32::from(b)).sum();
        mem[0..4].copy_from_slice(&total.to_le_bytes());
        out.get_mut(0..4)
            .ok_or(TeeError::BufferTooSmall)?
            .copy_from_slice(&total.to_le_bytes());
        Ok(4)
    }

    #[test]
    fn normal_world_cannot_touch_secure_memory() {
        let mut t = tee();
        assert_eq!(t.write(World::Normal, 0, &[1]), Err(TeeError::Denied));
        let mut out = [0u8; 1];
        assert_eq!(t.read(World::Normal, 0, &mut out), Err(TeeError::Denied));
    }

    #[test]
    fn secure_world_reads_and_writes_within_bounds() {
        let mut t = tee();
        t.write(World::Secure, 10, &[1, 2, 3]).unwrap();
        let mut out = [0u8; 3];
        t.read(World::Secure, 10, &mut out).unwrap();
        assert_eq!(out, [1, 2, 3]);
        assert_eq!(
            t.write(World::Secure, 62, &[0; 3]),
            Err(TeeError::OutOfRange)
        );
        assert_eq!(t.capacity(), 64);
    }

    #[test]
    fn a_registered_service_runs_against_secure_memory() {
        let mut t = tee();
        t.register(1, sum).unwrap();
        let mut out = [0u8; 8];
        let n = t.call(1, &[1, 2, 3], &mut out).unwrap();
        assert_eq!(&out[..n], &6u32.to_le_bytes());
        let mut check = [0u8; 4];
        t.read(World::Secure, 0, &mut check).unwrap();
        assert_eq!(u32::from_le_bytes(check), 6);
    }

    #[test]
    fn unknown_services_and_full_tables_are_errors() {
        let mut t: MockTee<64, 1> = MockTee::new(KEY, measure(b"x"));
        let mut out = [0u8; 4];
        assert_eq!(t.call(5, &[], &mut out), Err(TeeError::UnknownService));
        t.register(1, sum).unwrap();
        assert_eq!(t.register(2, sum), Err(TeeError::Full));
        t.register(1, sum).unwrap(); // replacing an existing id needs no slot
    }

    #[test]
    fn a_short_output_buffer_is_reported_by_the_service() {
        let mut t = tee();
        t.register(1, sum).unwrap();
        let mut out = [0u8; 2];
        assert_eq!(t.call(1, &[1], &mut out), Err(TeeError::BufferTooSmall));
    }

    #[test]
    fn a_quote_verifies_for_its_nonce_and_key() {
        let t = tee();
        let report = t.quote(&NONCE).unwrap();
        assert!(t.verify(&report, &NONCE));
        assert_eq!(report.measurement, measure(b"secure firmware v1"));
    }

    #[test]
    fn verification_rejects_a_changed_nonce_measurement_or_tag() {
        let t = tee();
        let report = t.quote(&NONCE).unwrap();
        assert!(!t.verify(&report, &[0; DIGEST_LEN]));

        let mut other_code = report;
        other_code.measurement = measure(b"secure firmware v2");
        assert!(!t.verify(&other_code, &NONCE));

        let mut forged = report;
        forged.tag[0] ^= 1;
        assert!(!t.verify(&forged, &NONCE));
    }

    #[test]
    fn a_report_from_another_key_does_not_verify() {
        let other: MockTee<64, 1> = MockTee::new([8; DIGEST_LEN], measure(b"secure firmware v1"));
        let report = other.quote(&NONCE).unwrap();
        assert!(!tee().verify(&report, &NONCE));
    }

    #[test]
    fn measurements_separate_different_images() {
        assert_ne!(measure(b"ab"), measure(b"ba"));
        assert_eq!(measure(b"same"), measure(b"same"));
    }
}

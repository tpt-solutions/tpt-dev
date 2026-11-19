//! The Keccak sponge, and the hash functions FIPS 202 builds on it.
//!
//! Only the functions ML-KEM needs are exposed: SHA3-256, SHA3-512, SHAKE128
//! and SHAKE256. The sponge is written from the specification, and is tested
//! against reference digests.

/// Round constants for Keccak-f[1600].
const RC: [u64; 24] = [
    0x0000_0000_0000_0001,
    0x0000_0000_0000_8082,
    0x8000_0000_0000_808a,
    0x8000_0000_8000_8000,
    0x0000_0000_0000_808b,
    0x0000_0000_8000_0001,
    0x8000_0000_8000_8081,
    0x8000_0000_0000_8009,
    0x0000_0000_0000_008a,
    0x0000_0000_0000_0088,
    0x0000_0000_8000_8009,
    0x0000_0000_8000_000a,
    0x0000_0000_8000_808b,
    0x8000_0000_0000_008b,
    0x8000_0000_0000_8089,
    0x8000_0000_0000_8003,
    0x8000_0000_0000_8002,
    0x8000_0000_0000_0080,
    0x0000_0000_0000_800a,
    0x8000_0000_8000_000a,
    0x8000_0000_8000_8081,
    0x8000_0000_0000_8080,
    0x0000_0000_8000_0001,
    0x8000_0000_8000_8008,
];

/// Rotation offsets, indexed by `x + 5 * y`.
const ROTATION: [u32; 25] = [
    0, 1, 62, 28, 27, //
    36, 44, 6, 55, 20, //
    3, 10, 43, 25, 39, //
    41, 45, 15, 21, 8, //
    18, 2, 61, 56, 14,
];

/// The Keccak-f[1600] permutation.
fn keccak_f(a: &mut [u64; 25]) {
    for &rc in &RC {
        // theta
        let mut c = [0u64; 5];
        for (x, cx) in c.iter_mut().enumerate() {
            *cx = a[x] ^ a[x + 5] ^ a[x + 10] ^ a[x + 15] ^ a[x + 20];
        }
        for x in 0..5 {
            let d = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
            for y in 0..5 {
                a[x + 5 * y] ^= d;
            }
        }
        // rho and pi
        let mut b = [0u64; 25];
        for x in 0..5 {
            for y in 0..5 {
                let i = x + 5 * y;
                b[y + 5 * ((2 * x + 3 * y) % 5)] = a[i].rotate_left(ROTATION[i]);
            }
        }
        // chi
        for y in 0..5 {
            for x in 0..5 {
                a[x + 5 * y] = b[x + 5 * y] ^ (!b[(x + 1) % 5 + 5 * y] & b[(x + 2) % 5 + 5 * y]);
            }
        }
        // iota
        a[0] ^= rc;
    }
}

/// A Keccak sponge: absorbs input, then squeezes output.
pub(crate) struct Sponge {
    state: [u64; 25],
    rate: usize,
    offset: usize,
}

impl Sponge {
    /// A sponge with the given rate in bytes: 136 for SHA3-256 and SHAKE256,
    /// 72 for SHA3-512, and 168 for SHAKE128.
    pub(crate) fn new(rate: usize) -> Self {
        Self {
            state: [0; 25],
            rate,
            offset: 0,
        }
    }

    /// Absorbs `data`.
    pub(crate) fn absorb(&mut self, data: &[u8]) {
        for &byte in data {
            self.state[self.offset / 8] ^= u64::from(byte) << ((self.offset % 8) * 8);
            self.offset += 1;
            if self.offset == self.rate {
                keccak_f(&mut self.state);
                self.offset = 0;
            }
        }
    }

    /// Pads the input with the domain-separation byte `domain`, and switches to squeezing.
    pub(crate) fn finish(&mut self, domain: u8) {
        self.state[self.offset / 8] ^= u64::from(domain) << ((self.offset % 8) * 8);
        let last = self.rate - 1;
        self.state[last / 8] ^= 0x80u64 << ((last % 8) * 8);
        keccak_f(&mut self.state);
        self.offset = 0;
    }

    /// Fills `out` with output bytes.
    pub(crate) fn squeeze(&mut self, out: &mut [u8]) {
        for byte in out {
            if self.offset == self.rate {
                keccak_f(&mut self.state);
                self.offset = 0;
            }
            *byte = (self.state[self.offset / 8] >> ((self.offset % 8) * 8)) as u8;
            self.offset += 1;
        }
    }
}

/// Hashes the concatenation of `parts` with a fixed-rate sponge.
fn hash_into(rate: usize, domain: u8, parts: &[&[u8]], out: &mut [u8]) {
    let mut sponge = Sponge::new(rate);
    for part in parts {
        sponge.absorb(part);
    }
    sponge.finish(domain);
    sponge.squeeze(out);
}

/// SHA3-256 of the concatenation of `parts`.
pub(crate) fn sha3_256(parts: &[&[u8]]) -> [u8; 32] {
    let mut out = [0u8; 32];
    hash_into(136, 0x06, parts, &mut out);
    out
}

/// SHA3-512 of the concatenation of `parts`.
pub(crate) fn sha3_512(parts: &[&[u8]]) -> [u8; 64] {
    let mut out = [0u8; 64];
    hash_into(72, 0x06, parts, &mut out);
    out
}

/// SHAKE256 of the concatenation of `parts`, filling `out`.
pub(crate) fn shake256(parts: &[&[u8]], out: &mut [u8]) {
    hash_into(136, 0x1f, parts, out);
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> std::string::String {
        use core::fmt::Write;
        let mut s = std::string::String::new();
        for b in bytes {
            write!(s, "{b:02x}").unwrap();
        }
        s
    }

    #[test]
    fn sha3_256_of_empty_input() {
        assert_eq!(
            hex(&sha3_256(&[])),
            "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"
        );
    }

    #[test]
    fn shake128_matches_reference_prefix() {
        let mut sponge = Sponge::new(168);
        sponge.absorb(b"abc");
        sponge.finish(0x1f);
        let mut out = [0u8; 16];
        sponge.squeeze(&mut out);
        assert_eq!(hex(&out), "5881092dd818bf5cf8a3ddb793fbcba7");
    }

    #[test]
    fn splitting_input_does_not_change_the_digest() {
        assert_eq!(
            sha3_512(&[b"hello ", b"world"]),
            sha3_512(&[b"hello world"])
        );
    }

    #[test]
    fn output_longer_than_one_block_is_consistent() {
        // Squeezing in pieces must match squeezing at once.
        let mut once = [0u8; 400];
        shake256(&[b"seed"], &mut once);
        assert_eq!(hex(&once[..20]), "4fd6800b5ddf65323de29f59e5da90d3fa677859");
        let mut pieces = [0u8; 400];
        let mut sponge = Sponge::new(136);
        sponge.absorb(b"seed");
        sponge.finish(0x1f);
        for chunk in pieces.chunks_mut(37) {
            sponge.squeeze(chunk);
        }
        assert_eq!(once, pieces);
    }
}

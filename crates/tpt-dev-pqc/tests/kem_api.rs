//! The public ML-KEM API: sizes, input checks, the RNG path, and tampering.

#![allow(clippy::unwrap_used)]

use tpt_dev_pqc::{
    Ciphertext, DecapsulationKey, EncapsulationKey, KemError, MlKem512, MlKem768, MlKem1024,
    Parameters, Rng, decapsulate, encapsulate, keygen,
};

/// A deterministic test generator. Production code must pass a CSPRNG.
struct Lcg(u64);

impl Rng for Lcg {
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for b in dest {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *b = (self.0 >> 56) as u8;
        }
    }
}

fn round_trip<P: Parameters>() {
    let mut rng = Lcg(42);
    let (ek, dk) = keygen::<P, _>(&mut rng);
    let (ct, sent) = encapsulate(&ek, &mut rng);
    let received = decapsulate(&dk, &ct);
    assert_eq!(sent.as_bytes(), received.as_bytes(), "{}", P::NAME);
}

#[test]
fn encapsulated_secret_is_recovered_at_every_level() {
    round_trip::<MlKem512>();
    round_trip::<MlKem768>();
    round_trip::<MlKem1024>();
}

#[test]
fn encoded_sizes_match_the_standard() {
    assert_eq!(EncapsulationKey::<MlKem512>::encoded_len(), 800);
    assert_eq!(DecapsulationKey::<MlKem512>::encoded_len(), 1632);
    assert_eq!(Ciphertext::<MlKem512>::encoded_len(), 768);
    assert_eq!(EncapsulationKey::<MlKem768>::encoded_len(), 1184);
    assert_eq!(DecapsulationKey::<MlKem768>::encoded_len(), 2400);
    assert_eq!(Ciphertext::<MlKem768>::encoded_len(), 1088);
    assert_eq!(EncapsulationKey::<MlKem1024>::encoded_len(), 1568);
    assert_eq!(DecapsulationKey::<MlKem1024>::encoded_len(), 3168);
    assert_eq!(Ciphertext::<MlKem1024>::encoded_len(), 1568);
}

#[test]
fn wrong_lengths_are_refused() {
    assert_eq!(
        EncapsulationKey::<MlKem768>::from_bytes(&[0; 10])
            .err()
            .unwrap(),
        KemError::InvalidLength
    );
    assert_eq!(
        DecapsulationKey::<MlKem768>::from_bytes(&[0; 10])
            .err()
            .unwrap(),
        KemError::InvalidLength
    );
    assert_eq!(
        Ciphertext::<MlKem512>::from_bytes(&[0; 10]).err().unwrap(),
        KemError::InvalidLength
    );
}

#[test]
fn an_encapsulation_key_with_an_unreduced_coefficient_is_refused() {
    let mut rng = Lcg(7);
    let (ek, _) = keygen::<MlKem512, _>(&mut rng);
    let mut bytes = ek.as_bytes().to_vec();
    // Set the first 12-bit coefficient to 0xfff, which is above q = 3329.
    bytes[0] = 0xff;
    bytes[1] |= 0x0f;
    assert_eq!(
        EncapsulationKey::<MlKem512>::from_bytes(&bytes)
            .err()
            .unwrap(),
        KemError::InvalidEncapsulationKey
    );
    // The untouched key is still accepted.
    assert!(EncapsulationKey::<MlKem512>::from_bytes(ek.as_bytes()).is_ok());
}

#[test]
fn a_tampered_ciphertext_gives_a_different_secret_without_an_error() {
    let mut rng = Lcg(99);
    let (ek, dk) = keygen::<MlKem768, _>(&mut rng);
    let (ct, sent) = encapsulate(&ek, &mut rng);
    let mut bytes = ct.as_bytes().to_vec();
    bytes[10] ^= 0x01;
    let tampered = Ciphertext::<MlKem768>::from_bytes(&bytes).unwrap();
    let rejected = decapsulate(&dk, &tampered);
    assert_ne!(rejected.as_bytes(), sent.as_bytes());
    // Decapsulating the same bad ciphertext twice gives the same secret.
    let again = decapsulate(&dk, &tampered);
    assert_eq!(rejected.as_bytes(), again.as_bytes());
}

#[test]
fn keys_from_different_seeds_do_not_agree() {
    let mut a = Lcg(1);
    let mut b = Lcg(2);
    let (ek_a, _) = keygen::<MlKem512, _>(&mut a);
    let (_, dk_b) = keygen::<MlKem512, _>(&mut b);
    let (ct, sent) = encapsulate(&ek_a, &mut a);
    let received = decapsulate(&dk_b, &ct);
    assert_ne!(sent.as_bytes(), received.as_bytes());
}

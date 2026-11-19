//! The public ML-DSA API: the randomized path, tampering, contexts, and lengths.

#![allow(clippy::unwrap_used)]

use tpt_dev_pqc::{
    DsaError, DsaParameters, MlDsa44, MlDsa65, MlDsa87, Rng, Signature, SigningKey, VerifyingKey,
    dsa_keygen as keygen, dsa_sign as sign, sign_deterministic, verify,
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

fn round_trip<P: DsaParameters>() {
    let mut rng = Lcg(5);
    let (vk, sk) = keygen::<P, _>(&mut rng);
    let sig = sign(&sk, b"hello", b"ctx", &mut rng).unwrap();
    assert!(verify(&vk, b"hello", b"ctx", &sig), "{}", P::NAME);
}

#[test]
fn hedged_signatures_verify_at_every_level() {
    round_trip::<MlDsa44>();
    round_trip::<MlDsa65>();
    round_trip::<MlDsa87>();
}

#[test]
fn hedged_signatures_differ_but_both_verify() {
    let mut rng = Lcg(11);
    let (vk, sk) = keygen::<MlDsa44, _>(&mut rng);
    let a = sign(&sk, b"same message", b"", &mut rng).unwrap();
    let b = sign(&sk, b"same message", b"", &mut rng).unwrap();
    assert_ne!(a.as_bytes(), b.as_bytes());
    assert!(verify(&vk, b"same message", b"", &a));
    assert!(verify(&vk, b"same message", b"", &b));
}

#[test]
fn deterministic_signing_repeats() {
    let mut rng = Lcg(3);
    let (vk, sk) = keygen::<MlDsa65, _>(&mut rng);
    let a = sign_deterministic(&sk, b"m", b"").unwrap();
    let b = sign_deterministic(&sk, b"m", b"").unwrap();
    assert_eq!(a.as_bytes(), b.as_bytes());
    assert!(verify(&vk, b"m", b"", &a));
}

#[test]
fn a_signature_fails_for_another_message_context_or_key() {
    let mut rng = Lcg(21);
    let (vk, sk) = keygen::<MlDsa44, _>(&mut rng);
    let (other_vk, _) = keygen::<MlDsa44, _>(&mut rng);
    let sig = sign_deterministic(&sk, b"msg", b"ctx").unwrap();
    assert!(verify(&vk, b"msg", b"ctx", &sig));
    assert!(!verify(&vk, b"msg2", b"ctx", &sig));
    assert!(!verify(&vk, b"msg", b"other", &sig));
    assert!(!verify(&other_vk, b"msg", b"ctx", &sig));
}

#[test]
fn a_tampered_signature_is_rejected() {
    let mut rng = Lcg(31);
    let (vk, sk) = keygen::<MlDsa87, _>(&mut rng);
    let sig = sign_deterministic(&sk, b"msg", b"").unwrap();
    // Flip a bit in the challenge hash, and another in the response vector.
    for pos in [0usize, 200] {
        let mut bytes = sig.as_bytes().to_vec();
        bytes[pos] ^= 0x04;
        let bad = Signature::<MlDsa87>::from_bytes(&bytes).unwrap();
        assert!(!verify(&vk, b"msg", b"", &bad), "flipped byte {pos}");
    }
}

#[test]
fn an_overlong_context_is_refused() {
    let mut rng = Lcg(41);
    let (_, sk) = keygen::<MlDsa44, _>(&mut rng);
    let long = [0u8; 256];
    assert_eq!(
        sign_deterministic(&sk, b"m", &long).err().unwrap(),
        DsaError::ContextTooLong
    );
    let max = [0u8; 255];
    assert!(sign_deterministic(&sk, b"m", &max).is_ok());
}

#[test]
fn encoded_sizes_match_the_standard() {
    assert_eq!(VerifyingKey::<MlDsa44>::encoded_len(), 1312);
    assert_eq!(SigningKey::<MlDsa44>::encoded_len(), 2560);
    assert_eq!(Signature::<MlDsa44>::encoded_len(), 2420);
    assert_eq!(VerifyingKey::<MlDsa65>::encoded_len(), 1952);
    assert_eq!(SigningKey::<MlDsa65>::encoded_len(), 4032);
    assert_eq!(Signature::<MlDsa65>::encoded_len(), 3309);
    assert_eq!(VerifyingKey::<MlDsa87>::encoded_len(), 2592);
    assert_eq!(SigningKey::<MlDsa87>::encoded_len(), 4896);
    assert_eq!(Signature::<MlDsa87>::encoded_len(), 4627);
}

#[test]
fn wrong_lengths_are_refused() {
    assert_eq!(
        VerifyingKey::<MlDsa44>::from_bytes(&[0; 3]).err().unwrap(),
        DsaError::InvalidLength
    );
    assert_eq!(
        SigningKey::<MlDsa44>::from_bytes(&[0; 3]).err().unwrap(),
        DsaError::InvalidLength
    );
    assert_eq!(
        Signature::<MlDsa44>::from_bytes(&[0; 3]).err().unwrap(),
        DsaError::InvalidLength
    );
}

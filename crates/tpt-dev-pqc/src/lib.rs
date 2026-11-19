#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod dsa;
mod keccak;
mod kem;
mod poly;

pub use dsa::{
    DsaError, MlDsa44, MlDsa65, MlDsa87, Parameters as DsaParameters, Signature, SigningKey,
    VerifyingKey, keygen as dsa_keygen, keygen_from_seed as dsa_keygen_from_seed, sign as dsa_sign,
    sign_deterministic, verify,
};
pub use kem::{
    Ciphertext, DecapsulationKey, EncapsulationKey, KemError, MlKem512, MlKem768, MlKem1024,
    Parameters, Rng, SHARED_SECRET_LEN, SharedSecret, decapsulate, encapsulate,
    encapsulate_from_seed, keygen, keygen_from_seed,
};

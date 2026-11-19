#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

mod keccak;
mod kem;
mod poly;

pub use kem::{
    Ciphertext, DecapsulationKey, EncapsulationKey, KemError, MlKem512, MlKem768, MlKem1024,
    Parameters, Rng, SHARED_SECRET_LEN, SharedSecret, decapsulate, encapsulate,
    encapsulate_from_seed, keygen, keygen_from_seed,
};

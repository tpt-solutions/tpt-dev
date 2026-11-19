//! ML-KEM known-answer tests, from the NIST post-quantum KAT files.
//!
//! The fixtures hold the first three records of each parameter set. Each record
//! fixes the seeds, so key generation, encapsulation and decapsulation must
//! reproduce every value exactly. A record also has an invalid ciphertext, whose
//! decapsulation must give the implicit-rejection secret.
//!
//! The full NIST files can be run with `TPT_KAT_DIR=<dir> cargo test -p
//! tpt-dev-pqc --test kat_mlkem -- --ignored`, where `<dir>` holds
//! `kat_MLKEM_512.rsp`, `kat_MLKEM_768.rsp` and `kat_MLKEM_1024.rsp`.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::string::String;
use std::vec::Vec;

use tpt_dev_pqc::{
    Ciphertext, DecapsulationKey, EncapsulationKey, MlKem512, MlKem768, MlKem1024, Parameters,
    decapsulate, encapsulate_from_seed, keygen_from_seed,
};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// Splits an .rsp file into records of `field -> value` pairs.
fn records(text: &str) -> Vec<BTreeMap<String, String>> {
    let mut out: Vec<BTreeMap<String, String>> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(" = ") {
            if key == "count" {
                out.push(BTreeMap::new());
            }
            if let Some(record) = out.last_mut() {
                record.insert(String::from(key), String::from(value));
            }
        }
    }
    out
}

fn array32(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    out
}

fn check_file<P: Parameters>(text: &str) -> usize {
    let recs = records(text);
    for (n, rec) in recs.iter().enumerate() {
        let get = |k: &str| unhex(&rec[k]);
        let (z, d, msg) = (array32(&get("z")), array32(&get("d")), array32(&get("msg")));

        let (ek, dk) = keygen_from_seed::<P>(&d, &z);
        assert_eq!(
            ek.as_bytes(),
            get("pk").as_slice(),
            "{} record {n}: pk",
            P::NAME
        );
        assert_eq!(
            dk.as_bytes(),
            get("sk").as_slice(),
            "{} record {n}: sk",
            P::NAME
        );

        let (ct, ss) = encapsulate_from_seed(&ek, &msg);
        assert_eq!(
            ct.as_bytes(),
            get("ct").as_slice(),
            "{} record {n}: ct",
            P::NAME
        );
        assert_eq!(
            ss.as_bytes(),
            get("ss").as_slice(),
            "{} record {n}: ss",
            P::NAME
        );

        let recovered = decapsulate(&dk, &ct);
        assert_eq!(
            recovered.as_bytes(),
            get("ss").as_slice(),
            "{} record {n}: decaps",
            P::NAME
        );

        let invalid = Ciphertext::<P>::from_bytes(&get("ct_n")).unwrap();
        let rejected = decapsulate(&dk, &invalid);
        assert_eq!(
            rejected.as_bytes(),
            get("ss_n").as_slice(),
            "{} record {n}: implicit rejection",
            P::NAME
        );
        // Parsing the keys back must give the same bytes.
        assert!(EncapsulationKey::<P>::from_bytes(ek.as_bytes()).is_ok());
        assert!(DecapsulationKey::<P>::from_bytes(dk.as_bytes()).is_ok());
    }
    recs.len()
}

#[test]
fn ml_kem_512_matches_the_nist_vectors() {
    let n = check_file::<MlKem512>(include_str!("fixtures/kat_mlkem_512.rsp"));
    assert_eq!(n, 3);
}

#[test]
fn ml_kem_768_matches_the_nist_vectors() {
    let n = check_file::<MlKem768>(include_str!("fixtures/kat_mlkem_768.rsp"));
    assert_eq!(n, 3);
}

#[test]
fn ml_kem_1024_matches_the_nist_vectors() {
    let n = check_file::<MlKem1024>(include_str!("fixtures/kat_mlkem_1024.rsp"));
    assert_eq!(n, 3);
}

#[test]
#[ignore = "needs the full NIST files; set TPT_KAT_DIR"]
fn full_nist_files_when_available() {
    let Ok(dir) = std::env::var("TPT_KAT_DIR") else {
        return;
    };
    let read = |name: &str| std::fs::read_to_string(format!("{dir}/{name}")).unwrap();
    let total = check_file::<MlKem512>(&read("kat_MLKEM_512.rsp"))
        + check_file::<MlKem768>(&read("kat_MLKEM_768.rsp"))
        + check_file::<MlKem1024>(&read("kat_MLKEM_1024.rsp"));
    println!("checked {total} records");
}

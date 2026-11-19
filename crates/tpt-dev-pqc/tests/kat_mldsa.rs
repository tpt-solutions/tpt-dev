//! ML-DSA known-answer tests, from the NIST post-quantum KAT files.
//!
//! The fixtures are the first three records of the deterministic, pure-mode
//! vectors for each parameter set. Each record fixes the key seed, so key
//! generation must reproduce the public and secret keys, and deterministic
//! signing must reproduce the signature.
//!
//! The full NIST files can be run with `TPT_KAT_DIR=<dir> cargo test -p
//! tpt-dev-pqc --test kat_mldsa -- --ignored`, where `<dir>` holds
//! `kat_MLDSA_{44,65,87}_det_pure.rsp`.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::string::String;
use std::vec::Vec;

use tpt_dev_pqc::{
    DsaParameters, MlDsa44, MlDsa65, MlDsa87, Signature, SigningKey, VerifyingKey,
    dsa_keygen_from_seed, sign_deterministic, verify,
};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

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

fn check_file<P: DsaParameters>(text: &str) -> usize {
    let recs = records(text);
    for (n, rec) in recs.iter().enumerate() {
        let get = |k: &str| unhex(&rec[k]);
        let mut xi = [0u8; 32];
        xi.copy_from_slice(&get("xi"));
        let msg = get("msg");
        let ctx = get("ctx");
        let sm = get("sm");
        let sig_len = sm.len() - msg.len();
        assert_eq!(
            sm[sig_len..],
            msg[..],
            "{} record {n}: sm carries msg",
            P::NAME
        );

        let (vk, sk) = dsa_keygen_from_seed::<P>(&xi);
        assert_eq!(
            vk.as_bytes(),
            get("pk").as_slice(),
            "{} record {n}: pk",
            P::NAME
        );
        assert_eq!(
            sk.as_bytes(),
            get("sk").as_slice(),
            "{} record {n}: sk",
            P::NAME
        );

        let sig = sign_deterministic(&sk, &msg, &ctx).unwrap();
        assert_eq!(
            sig.as_bytes(),
            &sm[..sig_len],
            "{} record {n}: signature",
            P::NAME
        );
        assert!(
            verify(&vk, &msg, &ctx, &sig),
            "{} record {n}: verify",
            P::NAME
        );

        let mut wrong = msg.clone();
        wrong.push(0);
        assert!(
            !verify(&vk, &wrong, &ctx, &sig),
            "{} record {n}: wrong message",
            P::NAME
        );
        let _ = (
            VerifyingKey::<P>::encoded_len(),
            SigningKey::<P>::encoded_len(),
        );
        let _ = Signature::<P>::from_bytes(&sm[..sig_len]).unwrap();
    }
    recs.len()
}

#[test]
fn ml_dsa_44_matches_the_nist_vectors() {
    assert_eq!(
        check_file::<MlDsa44>(include_str!("fixtures/kat_mldsa_44.rsp")),
        3
    );
}

#[test]
fn ml_dsa_65_matches_the_nist_vectors() {
    assert_eq!(
        check_file::<MlDsa65>(include_str!("fixtures/kat_mldsa_65.rsp")),
        3
    );
}

#[test]
fn ml_dsa_87_matches_the_nist_vectors() {
    assert_eq!(
        check_file::<MlDsa87>(include_str!("fixtures/kat_mldsa_87.rsp")),
        3
    );
}

#[test]
#[ignore = "needs the full NIST files; set TPT_KAT_DIR"]
fn full_nist_files_when_available() {
    let Ok(dir) = std::env::var("TPT_KAT_DIR") else {
        return;
    };
    let read = |name: &str| std::fs::read_to_string(format!("{dir}/{name}")).unwrap();
    let total = check_file::<MlDsa44>(&read("kat_MLDSA_44_det_pure.rsp"))
        + check_file::<MlDsa65>(&read("kat_MLDSA_65_det_pure.rsp"))
        + check_file::<MlDsa87>(&read("kat_MLDSA_87_det_pure.rsp"));
    println!("checked {total} records");
}

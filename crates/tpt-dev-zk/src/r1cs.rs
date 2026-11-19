//! Encoding a circuit in the binary `.r1cs` format.
//!
//! The layout follows the iden3 `.r1cs` description, version 1: a magic word,
//! the version, and three sections. Section 1 is the header, section 2 the
//! constraints, and section 3 the wire-to-label map. Wires are renumbered in the
//! order the format expects: the constant one, public outputs, public inputs,
//! private inputs, then internal wires. The field is BN254's scalar field, so
//! each field element takes 32 bytes.
//!
//! The encoder is checked by decoding its own output in the tests. It has not
//! been checked against circom or snarkjs in this environment.

use alloc::vec::Vec;

use crate::circuit::{Kind, State};
use crate::field::{F, MODULUS_LE};

/// Bytes per field element in the file.
const N8: u32 = 32;

/// The wire order the format expects, as the indices of the original wires.
fn order(state: &State) -> Vec<usize> {
    let mut out = Vec::with_capacity(state.kinds.len());
    for wanted in [
        Kind::One,
        Kind::Output,
        Kind::PublicInput,
        Kind::PrivateInput,
        Kind::Internal,
    ] {
        out.extend(
            state
                .kinds
                .iter()
                .enumerate()
                .filter(|(_, k)| **k == wanted)
                .map(|(i, _)| i),
        );
    }
    out
}

/// The witness in the format's wire order.
pub(crate) fn ordered_witness(state: &State) -> Vec<F> {
    order(state).into_iter().map(|i| state.values[i]).collect()
}

fn section(out: &mut Vec<u8>, kind: u32, body: &[u8]) {
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&(body.len() as u64).to_le_bytes());
    out.extend_from_slice(body);
}

pub(crate) fn encode(state: &State) -> Vec<u8> {
    let order = order(state);
    let mut new_index = alloc::vec![0u32; order.len()];
    for (new, &old) in order.iter().enumerate() {
        new_index[old] = new as u32;
    }
    let count = |kind: Kind| state.kinds.iter().filter(|k| **k == kind).count() as u32;
    let n_wires = order.len() as u32;

    let mut header = Vec::new();
    header.extend_from_slice(&N8.to_le_bytes());
    header.extend_from_slice(&MODULUS_LE);
    header.extend_from_slice(&n_wires.to_le_bytes());
    header.extend_from_slice(&count(Kind::Output).to_le_bytes());
    header.extend_from_slice(&count(Kind::PublicInput).to_le_bytes());
    header.extend_from_slice(&count(Kind::PrivateInput).to_le_bytes());
    header.extend_from_slice(&u64::from(n_wires).to_le_bytes());
    header.extend_from_slice(&(state.constraints.len() as u32).to_le_bytes());

    let mut constraints = Vec::new();
    for triple in &state.constraints {
        for lc in triple {
            constraints.extend_from_slice(&(lc.len() as u32).to_le_bytes());
            for &(wire, coef) in lc {
                constraints.extend_from_slice(&new_index[wire].to_le_bytes());
                constraints.extend_from_slice(&coef.to_bytes_le());
            }
        }
    }

    let mut labels = Vec::with_capacity(order.len() * 8);
    for &old in &order {
        labels.extend_from_slice(&(old as u64).to_le_bytes());
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"r1cs");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&3u32.to_le_bytes());
    section(&mut out, 1, &header);
    section(&mut out, 2, &constraints);
    section(&mut out, 3, &labels);
    out
}

/// The witness in the binary `.wtns` format, version 2: a header with the field
/// and the witness length, then one 32-byte value per wire in `.r1cs` order.
pub(crate) fn encode_witness(state: &State) -> Vec<u8> {
    let witness = ordered_witness(state);
    let mut header = Vec::new();
    header.extend_from_slice(&N8.to_le_bytes());
    header.extend_from_slice(&MODULUS_LE);
    header.extend_from_slice(&(witness.len() as u32).to_le_bytes());
    let mut values = Vec::with_capacity(witness.len() * N8 as usize);
    for v in &witness {
        values.extend_from_slice(&v.to_bytes_le());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"wtns");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    section(&mut out, 1, &header);
    section(&mut out, 2, &values);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::circuit::Circuit;

    /// A decoded file: the header fields, and the constraints as (wire, coefficient) lists.
    struct Decoded {
        n_wires: u32,
        n_pub_out: u32,
        n_pub_in: u32,
        n_prv_in: u32,
        constraints: Vec<Triple>,
    }

    /// One constraint: the three linear combinations, as (wire, coefficient) lists.
    type Triple = [Vec<(u32, [u8; 32])>; 3];

    fn u32_at(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    fn u64_at(b: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
    }

    fn decode(bytes: &[u8]) -> Decoded {
        assert_eq!(&bytes[0..4], b"r1cs");
        assert_eq!(u32_at(bytes, 4), 1);
        assert_eq!(u32_at(bytes, 8), 3);
        let mut at = 12;
        let mut sections = Vec::new();
        for _ in 0..3 {
            let kind = u32_at(bytes, at);
            let size = u64_at(bytes, at + 4) as usize;
            sections.push((kind, &bytes[at + 12..at + 12 + size]));
            at += 12 + size;
        }
        assert_eq!(at, bytes.len());
        let (k1, header) = sections[0];
        assert_eq!(k1, 1);
        // Header layout: n8 (4), prime (32), nWires (4), nPubOut (4), nPubIn (4),
        // nPrvIn (4), nLabels (8), nConstraints (4).
        assert_eq!(u32_at(header, 0), N8);
        assert_eq!(&header[4..36], &MODULUS_LE[..]);
        let n_wires = u32_at(header, 36);
        let n_pub_out = u32_at(header, 40);
        let n_pub_in = u32_at(header, 44);
        let n_prv_in = u32_at(header, 48);
        let n_constraints = u32_at(header, 60) as usize;

        let body = sections[1].1;
        let mut pos = 0;
        let mut constraints = Vec::new();
        for _ in 0..n_constraints {
            let mut triple: [Vec<(u32, [u8; 32])>; 3] = [Vec::new(), Vec::new(), Vec::new()];
            for lc in triple.iter_mut() {
                let terms = u32_at(body, pos) as usize;
                pos += 4;
                for _ in 0..terms {
                    let wire = u32_at(body, pos);
                    let mut coef = [0u8; 32];
                    coef.copy_from_slice(&body[pos + 4..pos + 36]);
                    lc.push((wire, coef));
                    pos += 36;
                }
            }
            constraints.push(triple);
        }
        assert_eq!(pos, body.len());
        assert_eq!(sections[2].1.len(), n_wires as usize * 8);
        Decoded {
            n_wires,
            n_pub_out,
            n_pub_in,
            n_prv_in,
            constraints,
        }
    }

    fn eval_decoded(witness: &[F], lc: &[(u32, [u8; 32])]) -> F {
        lc.iter().fold(F::ZERO, |acc, (i, c)| {
            acc + F::from_bytes_le(c).unwrap() * witness[*i as usize]
        })
    }

    #[test]
    fn the_witness_file_has_the_field_and_the_witness_length() {
        let c = Circuit::new();
        let x = c.private_input(3);
        let _ = c.output(&(x.clone() * x));
        let w = c.to_wtns();
        assert_eq!(&w[0..4], b"wtns");
        assert_eq!(u32_at(&w, 4), 2);
        assert_eq!(u32_at(&w, 8), 2);
        // Section 1 begins at byte 12: type, then its size, then n8 and the prime.
        assert_eq!(u32_at(&w, 12), 1);
        assert_eq!(u32_at(&w, 24), N8);
        assert_eq!(&w[28..60], &MODULUS_LE[..]);
        assert_eq!(u32_at(&w, 60), c.num_wires() as u32);
    }

    #[test]
    fn decoded_constraints_hold_for_the_encoded_witness() {
        let c = Circuit::new();
        let x = c.private_input(3);
        let y = c.public_input(4);
        let z = x.clone() * x.clone() + y.clone();
        let out = c.output(&z);
        c.assert_eq(&y, &y);
        assert_eq!(c.value(out), 13);
        c.check().unwrap();

        let decoded = decode(&c.to_r1cs());
        assert_eq!(decoded.n_wires as usize, c.num_wires());
        assert_eq!(decoded.n_pub_out, 1);
        assert_eq!(decoded.n_pub_in, 1);
        assert_eq!(decoded.n_prv_in, 1);
        assert_eq!(decoded.constraints.len(), c.num_constraints());

        let witness = c.witness();
        for triple in &decoded.constraints {
            let a = eval_decoded(&witness, &triple[0]);
            let b = eval_decoded(&witness, &triple[1]);
            let cc = eval_decoded(&witness, &triple[2]);
            assert_eq!(a * b, cc);
        }
    }
}

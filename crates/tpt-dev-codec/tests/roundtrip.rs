//! Round trips: whatever fits the layout decodes back to the same values.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_codec::{CodecError, Field, Layout};
use tpt_dev_test::{Config, Generator, IntRange, Pair, Rng, Seed, VecOf, check};

const FIELDS: [Field; 5] = [
    Field::bits("flag", 0, 1),
    Field::bits("mode", 1, 3),
    Field::bits("count", 4, 12),
    Field::be("temp", 16, 16),
    Field::be("id", 32, 32),
];
const LAYOUT: Layout<8> = Layout::new(&FIELDS);

fn masked(values: &[i64]) -> Vec<u64> {
    LAYOUT
        .fields()
        .iter()
        .zip(values)
        .map(|(f, &v)| {
            let mask = if f.width == 64 {
                u64::MAX
            } else {
                (1u64 << f.width) - 1
            };
            (v as u64) & mask
        })
        .collect()
}

#[test]
fn random_values_round_trip() {
    let source = VecOf::new(IntRange::new(0, i64::MAX), FIELDS.len());
    let config = Config {
        cases: 2000,
        seed: Seed(77),
        ..Config::default()
    };
    let result = check(&config, &source, |raw: &Vec<i64>| {
        if raw.len() != FIELDS.len() {
            return true;
        }
        let values = masked(raw);
        let mut buf = [0u8; 8];
        LAYOUT.encode(&values, &mut buf).unwrap();
        let mut out = [0u64; 5];
        LAYOUT.decode(&buf, &mut out).unwrap();
        out.as_slice() == values.as_slice()
    });
    assert!(result.is_ok(), "round trip failed: {:?}", result.err());
}

#[test]
fn bits_outside_fields_stay_zero() {
    // Bits 4..8 belong to no field, so they must be zero whatever the buffer held before.
    const GAPPED: Layout<2> = Layout::new(&[Field::bits("a", 0, 4), Field::bits("b", 8, 8)]);
    let mut buf = [0xFFu8; 2];
    GAPPED.encode(&[0xF, 0xAB], &mut buf).unwrap();
    assert_eq!(buf, [0x0F, 0xAB]);
}

#[test]
fn big_endian_bytes_are_in_order() {
    let mut buf = [0u8; 8];
    LAYOUT
        .encode(&[0, 0, 0, 0x1234, 0xAABBCCDD], &mut buf)
        .unwrap();
    assert_eq!(&buf[2..4], &[0x12, 0x34]);
    assert_eq!(&buf[4..8], &[0xAA, 0xBB, 0xCC, 0xDD]);
}

#[test]
fn errors_are_reported() {
    let mut buf = [0u8; 8];
    assert_eq!(
        LAYOUT.encode(&[0, 0, 0, 0], &mut buf),
        Err(CodecError::WrongValueCount {
            expected: 5,
            got: 4
        })
    );
    assert_eq!(
        LAYOUT.encode(&[2, 0, 0, 0, 0], &mut buf),
        Err(CodecError::TooWide { field: 0, width: 1 })
    );
    assert_eq!(
        LAYOUT.encode(&[0, 0, 0, 0, 0], &mut [0u8; 4]),
        Err(CodecError::BufferTooSmall { needed: 8 })
    );
}

#[test]
fn pair_generator_is_available_for_structured_cases() {
    // Keeps the generator API in use alongside the layout checks.
    let g = Pair(IntRange::new(0, 1), IntRange::new(0, 7));
    let v = g.generate(&mut Rng::new(Seed(3)), 1);
    assert!(v.0 <= 1 && v.1 <= 7);
}

#![allow(clippy::unwrap_used)]
//! Inputs much larger than the stack buffers, fed in many chunk sizes.

use tpt_dev_ast::{Event, ParseError, Stream, parse_all};

/// Builds a JSON document with `items` objects of a few fields each.
fn document(items: usize) -> Vec<u8> {
    let mut s = String::from("{\"items\": [");
    for i in 0..items {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!(
            "{{\"id\": {i}, \"name\": \"item-{i}\", \"ok\": {}, \"tags\": [\"a\", null, -{i}.5e2]}}",
            i % 2 == 0
        ));
    }
    s.push_str("]}");
    s.into_bytes()
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Summary {
    events: usize,
    keys: usize,
    numbers: usize,
    strings: usize,
    max_depth: usize,
}

fn summarise_chunked<const D: usize, const C: usize>(
    chunks: &[&[u8]],
) -> Result<Summary, ParseError> {
    let mut s: Stream<D, C> = Stream::new();
    let mut sum = Summary::default();
    let mut depth = 0usize;
    let mut visit = |e: Event<'_>| {
        sum.events += 1;
        match e {
            Event::ObjectStart | Event::ArrayStart => {
                depth += 1;
                sum.max_depth = sum.max_depth.max(depth);
            }
            Event::ObjectEnd | Event::ArrayEnd => depth -= 1,
            Event::Key(_) => sum.keys += 1,
            Event::Number(_) => sum.numbers += 1,
            Event::String(_) => sum.strings += 1,
            _ => {}
        }
    };
    for c in chunks {
        s.feed(c, &mut visit)?;
    }
    s.finish(&mut visit)?;
    Ok(sum)
}

#[test]
fn large_input_matches_across_chunk_sizes() {
    // About 100 KB of JSON, far more than the 64-byte carry buffer.
    let doc = document(1_000);
    assert!(doc.len() > 50_000);

    let whole = summarise_chunked::<4, 64>(&[&doc]).unwrap();
    // Four keys per item, plus the top-level "items" key.
    assert_eq!(whole.keys, 4_001);
    // Per item: the `id` and `-N.5e2` numbers, and the `"item-N"` and `"a"` strings.
    assert_eq!(whole.numbers, 2_000);
    assert_eq!(whole.strings, 2_000);

    for size in [1usize, 2, 3, 7, 16, 61, 64, 1000, 4096] {
        let chunks: Vec<&[u8]> = doc.chunks(size).collect();
        let got = summarise_chunked::<4, 64>(&chunks).unwrap();
        assert_eq!(got, whole, "chunk size {size}");
    }
}

#[test]
fn deep_nesting_is_rejected_without_panic() {
    let mut doc = Vec::new();
    doc.extend(std::iter::repeat_n(b'[', 40));
    doc.extend(std::iter::repeat_n(b']', 40));
    let r = parse_all::<8, 16>(&doc, &mut |_: Event<'_>| {});
    assert_eq!(
        r,
        Err(ParseError::DepthExceeded {
            offset: 8,
            limit: 8
        })
    );
    assert!(parse_all::<40, 16>(&doc, &mut |_: Event<'_>| {}).is_ok());
}

#[test]
fn stack_usage_does_not_grow_with_input() {
    // Same depth, far more tokens: the parser still needs only DEPTH slots.
    let doc = document(5_000);
    assert!(parse_all::<4, 32>(&doc, &mut |_: Event<'_>| {}).is_ok());
}

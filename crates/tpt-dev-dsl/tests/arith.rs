//! An arithmetic grammar, as a reference for writing grammars with this crate.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_dsl::{Expected, Expr, Grammar, Rule};

const DIGIT: Expr = Expr::Range(b'0', b'9');
const SPACE: Expr = Expr::Repeat(&Expr::Lit(b" "));

/// Rules, by index: 0 expr, 1 term, 2 factor, 3 number, 4 space.
const ARITH: Grammar = Grammar::new(&[
    Rule {
        name: "expr",
        body: Expr::Seq(&[
            Expr::Call(1),
            Expr::Repeat(&Expr::Seq(&[
                Expr::Call(4),
                Expr::Choice(&[Expr::Lit(b"+"), Expr::Lit(b"-")]),
                Expr::Call(4),
                Expr::Call(1),
            ])),
        ]),
    },
    Rule {
        name: "term",
        body: Expr::Seq(&[
            Expr::Call(2),
            Expr::Repeat(&Expr::Seq(&[
                Expr::Call(4),
                Expr::Choice(&[Expr::Lit(b"*"), Expr::Lit(b"/")]),
                Expr::Call(4),
                Expr::Call(2),
            ])),
        ]),
    },
    Rule {
        name: "factor",
        body: Expr::Choice(&[
            Expr::Call(3),
            Expr::Seq(&[
                Expr::Lit(b"("),
                Expr::Call(4),
                Expr::Call(0),
                Expr::Call(4),
                Expr::Lit(b")"),
            ]),
        ]),
    },
    Rule {
        name: "number",
        body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
    },
    Rule {
        name: "space",
        body: SPACE,
    },
]);

const EXPR: usize = 0;
const NUMBER: usize = 3;

fn count_rule(t: &tpt_dev_dsl::Tree, rule: usize) -> usize {
    let own = usize::from(t.rule == rule);
    own + t
        .children
        .iter()
        .map(|c| count_rule(c, rule))
        .sum::<usize>()
}

#[test]
fn accepts_a_nested_expression() {
    assert!(ARITH.recognise(EXPR, b"1 + 2 * (3 - 4)"));
    assert!(ARITH.recognise(EXPR, b"((12))/3"));
}

#[test]
fn builds_a_tree_with_one_node_per_number() {
    let tree = ARITH.parse(EXPR, b"1 + 22 * (3 - 4)").unwrap();
    assert_eq!(tree.rule, EXPR);
    assert_eq!((tree.start, tree.end), (0, 16));
    assert_eq!(count_rule(&tree, NUMBER), 4);
}

#[test]
fn reports_where_the_input_stops_making_sense() {
    let e = ARITH.parse(EXPR, b"1 +").unwrap_err();
    assert_eq!(e.position, 3);
    assert_eq!(e.expected, Expected::Literal(b"("));
}

#[test]
fn reports_trailing_input() {
    let e = ARITH.parse(EXPR, b"1 + 2)").unwrap_err();
    assert_eq!(e.expected, Expected::EndOfInput);
    assert_eq!(e.position, 5);
}

#[test]
fn very_deep_nesting_is_an_error_not_a_crash() {
    let mut input = Vec::new();
    input.extend(std::iter::repeat_n(b'(', 200));
    input.push(b'1');
    input.extend(std::iter::repeat_n(b')', 200));
    assert!(!ARITH.recognise(EXPR, &input));
    assert!(ARITH.parse(EXPR, &input).is_err());
}

#[test]
fn rule_names_resolve() {
    assert_eq!(ARITH.rule_index("factor"), Some(2));
    assert_eq!(ARITH.rule_index("missing"), None);
}

/// A small xorshift generator, so the corpus is the same on every run.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Random input over the arithmetic alphabet, with spaces, invalid tokens and
/// stray brackets.
fn random_arith(rng: &mut XorShift) -> Vec<u8> {
    const ALPHABET: &[u8] = b"0123456789   +-*/()x";
    let len = rng.below(24);
    (0..len)
        .map(|_| ALPHABET[rng.below(ALPHABET.len())])
        .collect()
}

/// Nested parentheses of random depth around the depth limit, with random
/// operators and spaces. These exercise the depth checks in the packrat memo.
fn random_nested(rng: &mut XorShift) -> Vec<u8> {
    // Each level costs three rule calls (expr, term, factor), so the depth
    // limit is reached at about 42 levels.
    let depth = 36 + rng.below(12);
    let mut out = Vec::new();
    for _ in 0..depth {
        out.extend_from_slice(if rng.below(3) == 0 {
            &b"( "[..]
        } else {
            &b"("[..]
        });
    }
    out.push(b'1');
    for _ in 0..depth {
        if rng.below(20) == 0 {
            out.push(b'x');
        }
        out.push(b')');
        match rng.below(4) {
            0 => out.extend_from_slice(b" + 1"),
            1 => out.extend_from_slice(b"*2"),
            _ => {}
        }
    }
    out
}

/// Checks that the packrat matcher agrees with the interpreter on `input`:
/// the same tree or the same error, and the same accept/reject.
fn assert_packrat_agrees(input: &[u8], memo: &mut tpt_dev_dsl::Memo<8192>) {
    memo.clear();
    let interpreted = ARITH.parse(EXPR, input);
    let packrat = ARITH.parse_packrat(EXPR, input, memo);
    match (&interpreted, &packrat) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "trees differ on {input:?}"),
        (Err(a), Err(tpt_dev_dsl::PackratError::Parse(b))) => {
            assert_eq!(a, b, "errors differ on {input:?}");
        }
        _ => panic!("results differ on {input:?}: {interpreted:?} vs {packrat:?}"),
    }
    memo.clear();
    assert_eq!(
        ARITH.recognise(EXPR, input),
        ARITH.recognise_packrat(EXPR, input, memo).unwrap(),
        "recognise differs on {input:?}"
    );
}

#[test]
fn packrat_matches_interpreter_on_random_corpus() {
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut memo = Box::new(tpt_dev_dsl::Memo::<8192>::new());
    let mut accepted = 0;
    for _ in 0..6000 {
        let input = random_arith(&mut rng);
        if ARITH.recognise(EXPR, &input) {
            accepted += 1;
        }
        assert_packrat_agrees(&input, &mut memo);
    }
    // The corpus must contain accepted inputs as well as rejected ones.
    assert!(accepted > 100, "only {accepted} accepted inputs");
}

#[test]
fn packrat_matches_interpreter_near_the_depth_limit() {
    let mut rng = XorShift(0x0123_4567_89AB_CDEF);
    let mut memo = Box::new(tpt_dev_dsl::Memo::<8192>::new());
    let mut accepted = 0;
    for _ in 0..800 {
        let input = random_nested(&mut rng);
        if ARITH.recognise(EXPR, &input) {
            accepted += 1;
        }
        assert_packrat_agrees(&input, &mut memo);
    }
    assert!(
        accepted > 50 && accepted < 750,
        "accepted {accepted} of 800"
    );
}

#[test]
fn packrat_reports_memo_full_instead_of_a_wrong_answer() {
    let mut memo = tpt_dev_dsl::Memo::<4>::new();
    let input = b"((1 + 2) * (3 - 4)) / (5 + 6)";
    let packrat = ARITH.parse_packrat(EXPR, input, &mut memo);
    assert_eq!(packrat, Err(tpt_dev_dsl::PackratError::MemoFull));
    memo.clear();
    assert_eq!(
        ARITH.recognise_packrat(EXPR, input, &mut memo),
        Err(tpt_dev_dsl::MemoFull)
    );
}

// Streaming: chunked input through `PackratStream` must match the one-shot
// packrat matcher on the same bytes.

use tpt_dev_dsl::{Memo, PackratError, PackratStream, StreamError, Tree};

/// The bytes that the arithmetic grammar can match.
const ARITH_ALPHABET: &[u8] = b"0123456789 +-*/()";

/// Feeds `input` in chunks whose sizes cycle through `sizes`, then finishes.
fn stream_parse<const N: usize>(
    input: &[u8],
    sizes: &[usize],
    memo: &mut Memo<N>,
) -> Result<Tree, StreamError> {
    let mut s: PackratStream<8192> = PackratStream::new(ARITH, EXPR);
    let (mut at, mut i) = (0, 0);
    while at < input.len() {
        let n = sizes[i % sizes.len()].min(input.len() - at);
        s.feed(&input[at..at + n])?;
        at += n;
        i += 1;
    }
    s.finish(memo)
}

/// Random chunk sizes: mostly small, sometimes 64 bytes.
fn random_chunking(rng: &mut XorShift) -> Vec<usize> {
    (0..=rng.below(4))
        .map(|_| match rng.below(4) {
            0 => 64,
            _ => 1 + rng.below(7),
        })
        .collect()
}

/// Random input: arithmetic bytes, plus invalid ones. Those are `x`, a
/// two-byte `é` (which is split across chunks), and a stray `0xFF`.
fn random_streamed(rng: &mut XorShift) -> Vec<u8> {
    let len = rng.below(40);
    let mut out = Vec::new();
    while out.len() < len {
        match rng.below(24) {
            0 => out.extend_from_slice("é".as_bytes()),
            1 => out.push(b'x'),
            2 => out.push(0xFF),
            _ => out.push(ARITH_ALPHABET[rng.below(ARITH_ALPHABET.len())]),
        }
    }
    out
}

#[test]
fn streaming_matches_one_shot_on_random_corpus() {
    let mut rng = XorShift(0x5EED_0001_2345_6789);
    let mut memo = Box::new(Memo::<8192>::new());
    let mut accepted = 0;
    for _ in 0..3000 {
        let input = random_streamed(&mut rng);
        let sizes = random_chunking(&mut rng);

        memo.clear();
        let one_shot = ARITH.parse_packrat(EXPR, &input, &mut memo);
        memo.clear();
        let streamed = stream_parse(&input, &sizes, &mut memo);

        match input.iter().position(|b| !ARITH_ALPHABET.contains(b)) {
            // A byte outside the alphabet is rejected at its own offset. The
            // one-shot matcher must reject the same input.
            Some(bad) => {
                assert_eq!(
                    streamed,
                    Err(StreamError::Invalid { offset: bad }),
                    "{input:?} {sizes:?}"
                );
                assert!(one_shot.is_err(), "{input:?}");
            }
            None => match (&one_shot, &streamed) {
                (Ok(a), Ok(b)) => {
                    assert_eq!(a, b, "trees differ on {input:?} {sizes:?}");
                    accepted += 1;
                }
                (Err(PackratError::Parse(a)), Err(StreamError::Parse(b))) => {
                    assert_eq!(a, b, "errors differ on {input:?} {sizes:?}");
                }
                (Err(PackratError::MemoFull), Err(StreamError::MemoFull)) => {}
                _ => panic!("results differ on {input:?} {sizes:?}: {one_shot:?} vs {streamed:?}"),
            },
        }
    }
    assert!(accepted > 100, "only {accepted} accepted inputs");
}

#[test]
fn streamed_recognise_matches_one_shot() {
    let mut rng = XorShift(0x0BAD_CAFE_0000_0001);
    let mut memo = Box::new(Memo::<8192>::new());
    for _ in 0..1000 {
        let input = random_streamed(&mut rng);
        let sizes = random_chunking(&mut rng);
        memo.clear();
        let expected = ARITH.recognise_packrat(EXPR, &input, &mut memo);
        let mut s: PackratStream<8192> = PackratStream::new(ARITH, EXPR);
        let mut at = 0;
        let mut streamed = Ok(());
        for n in sizes.iter().cycle() {
            if at >= input.len() {
                break;
            }
            let end = (at + n).min(input.len());
            streamed = s.feed(&input[at..end]);
            if streamed.is_err() {
                break;
            }
            at = end;
        }
        memo.clear();
        let got = match streamed {
            Ok(()) => s.finish_recognise(&mut memo),
            Err(e) => Err(e),
        };
        match (expected, got) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "{input:?}"),
            (Ok(false), Err(StreamError::Invalid { .. })) => {}
            other => panic!("{input:?}: {other:?}"),
        }
    }
}

#[test]
fn split_multi_byte_character_is_one_invalid_token_at_its_start() {
    let input = "1 + é".as_bytes();
    assert!(
        ARITH
            .parse_packrat(EXPR, input, &mut Memo::<64>::new())
            .is_err()
    );
    // The split falls inside `é`, either right after its lead byte or after the whole character.
    for split in [5, 6] {
        let mut s: PackratStream<64> = PackratStream::new(ARITH, EXPR);
        let r = s
            .feed(&input[..split])
            .and_then(|()| s.feed(&input[split..]));
        assert_eq!(
            r,
            Err(StreamError::Invalid { offset: 4 }),
            "split at {split}"
        );
    }
}

#[test]
fn grammar_with_a_multi_byte_literal_matches_across_a_split() {
    const ACCENT: Grammar = Grammar::new(&[Rule {
        name: "word",
        body: Expr::Seq(&[Expr::Lit("é".as_bytes()), Expr::Lit(b"x")]),
    }]);
    let input = "éx".as_bytes();
    let whole = ACCENT.parse(0, input).unwrap();
    for split in 1..input.len() {
        let mut s: PackratStream<16> = PackratStream::new(ACCENT, 0);
        s.feed(&input[..split]).unwrap();
        s.feed(&input[split..]).unwrap();
        let tree = s.finish(&mut Memo::<16>::new()).unwrap();
        assert_eq!(tree, whole, "split at {split}");
    }
}

#[test]
fn memo_overflow_is_reported_through_the_stream() {
    let input = b"((1 + 2) * (3 - 4)) / (5 + 6)";
    let r = stream_parse(input, &[3], &mut Memo::<4>::new());
    assert_eq!(r, Err(StreamError::MemoFull));
}

#[test]
fn an_error_is_sticky() {
    let mut s: PackratStream<16> = PackratStream::new(ARITH, EXPR);
    assert_eq!(s.feed(b"1 x"), Err(StreamError::Invalid { offset: 2 }));
    assert_eq!(s.feed(b"+2"), Err(StreamError::Invalid { offset: 2 }));
}

#[test]
fn one_megabyte_through_64_byte_chunks_stops_at_the_token_buffer() {
    // A fixed 4 KiB terminal buffer on the stack. The input is 1 MiB, and it
    // is valid, so the stream must stop when the buffer is full. It must not
    // keep reading.
    let mut s: PackratStream<4096> = PackratStream::new(ARITH, EXPR);
    let chunk = b"1+".repeat(32);
    let mut chunks_read = 0;
    let err = loop {
        match s.feed(&chunk) {
            Ok(()) => {
                chunks_read += 1;
                assert!(chunks_read < (1 << 20) / 64, "the buffer never filled");
            }
            Err(e) => break e,
        }
    };
    assert_eq!(err, StreamError::TokensFull { offset: 4096 });
    assert_eq!(chunks_read, 4096 / 64);
}

#[test]
fn large_input_within_the_token_buffer_matches_one_shot() {
    // `Memo::new` builds the table on the stack before it is boxed, so this
    // test runs on a thread with a stack large enough for a 65536-entry memo.
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let mut input = b"1".to_vec();
            for _ in 0..1500 {
                input.extend_from_slice(b"+1");
            }
            let mut memo = Box::new(Memo::<65536>::new());
            memo.clear();
            let one_shot = ARITH.parse_packrat(EXPR, &input, &mut memo);
            memo.clear();
            let streamed = stream_parse(&input, &[64], &mut memo);
            let one_shot = one_shot.unwrap();
            let streamed = streamed.unwrap();
            assert_eq!(one_shot, streamed);
        })
        .unwrap();
    handle.join().unwrap();
}

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

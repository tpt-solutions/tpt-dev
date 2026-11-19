//! The `dsl!` macro: the arithmetic grammar written in PEG notation expands to
//! the same grammar value as the one written by hand in `arith.rs`.

#![cfg(feature = "macros")]
#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_dsl::{Expected, Expr, Grammar, Rule, dsl};

dsl! {
    pub ARITH_DSL {
        expr = term (space ("+" / "-") space term)*;
        term = factor (space ("*" / "/") space factor)*;
        factor = number / "(" space expr space ")";
        number = ['0'-'9'] ['0'-'9']*;
        space = " "*;
    }
}

const DIGIT: Expr = Expr::Range(b'0', b'9');
const SPACE: Expr = Expr::Repeat(&Expr::Lit(b" "));

/// The same grammar, written by hand, as in `arith.rs`.
const ARITH_BY_HAND: Grammar = Grammar::new(&[
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

#[test]
fn macro_grammar_equals_the_hand_written_grammar() {
    assert_eq!(ARITH_DSL, ARITH_BY_HAND);
}

#[test]
fn macro_grammar_parses_like_the_hand_written_one() {
    for input in [
        &b"1 + 22 * (3 - 4)"[..],
        b"((12))/3",
        b"1 +",
        b"1 + 2)",
        b"",
    ] {
        assert_eq!(
            ARITH_DSL.parse(0, input),
            ARITH_BY_HAND.parse(0, input),
            "input {input:?}"
        );
    }
    let e = ARITH_DSL.parse(0, b"1 +").unwrap_err();
    assert_eq!(e.position, 3);
    assert_eq!(e.expected, Expected::Literal(b"("));
}

#[test]
fn macro_grammar_is_usable_with_packrat() {
    let mut memo = tpt_dev_dsl::Memo::<256>::new();
    assert!(
        ARITH_DSL
            .recognise_packrat(0, b"1 + 2 * (3 - 4)", &mut memo)
            .unwrap()
    );
    assert_eq!(ARITH_DSL.rule_index("factor"), Some(2));
}

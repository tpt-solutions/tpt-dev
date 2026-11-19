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

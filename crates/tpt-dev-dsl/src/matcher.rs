//! The interpreter: matches input against a grammar, and builds a parse tree.

use alloc::vec::Vec;
use core::fmt;

use crate::grammar::{Expr, Grammar};

/// How deeply rules may nest before matching gives up. This stops left
/// recursion from overflowing the stack.
pub const MAX_DEPTH: usize = 128;

/// A node of the parse tree: a rule matched a span of the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    /// Index of the rule in the grammar.
    pub rule: usize,
    /// Byte offset where the match starts.
    pub start: usize,
    /// Byte offset just past the match.
    pub end: usize,
    /// The rules matched inside this one, in order.
    pub children: Vec<Tree>,
}

/// What the parser was looking for when it failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expected {
    /// These exact bytes.
    Literal(&'static [u8]),
    /// One byte in this inclusive range.
    Range(u8, u8),
    /// The end of the input.
    EndOfInput,
    /// A rule nested too deeply, which usually means left recursion.
    Recursion,
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expected::Literal(bytes) => {
                write!(f, "{:?}", core::str::from_utf8(bytes).unwrap_or("<bytes>"))
            }
            Expected::Range(lo, hi) => write!(f, "a byte from {lo:#04x} to {hi:#04x}"),
            Expected::EndOfInput => f.write_str("end of input"),
            Expected::Recursion => f.write_str("a rule that consumes no input on recursion"),
        }
    }
}

/// A parse that did not match the whole input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// The furthest byte offset the parser reached.
    pub position: usize,
    /// What it was looking for there.
    pub expected: Expected,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at byte {}: expected {}", self.position, self.expected)
    }
}

struct Ctx<'g, 'i> {
    grammar: &'g Grammar,
    input: &'i [u8],
    build: bool,
    furthest: usize,
    expected: Expected,
}

impl Ctx<'_, '_> {
    fn fail(&mut self, pos: usize, what: Expected) {
        if pos >= self.furthest {
            self.furthest = pos;
            self.expected = what;
        }
    }
}

impl Grammar {
    /// Parses all of `input` with rule `rule`, and returns the parse tree.
    ///
    /// Needs `alloc` for the tree. Use [`Grammar::recognise`] to check input without one.
    pub fn parse(&self, rule: usize, input: &[u8]) -> Result<Tree, ParseError> {
        let mut ctx = Ctx {
            grammar: self,
            input,
            build: true,
            furthest: 0,
            expected: Expected::EndOfInput,
        };
        let mut kids = Vec::new();
        match eval_call(&mut ctx, rule, 0, 0, &mut kids) {
            Some(end) if end == input.len() => Ok(kids.pop().unwrap_or(Tree {
                rule,
                start: 0,
                end,
                children: Vec::new(),
            })),
            // A prefix matched. Report the furthest failure if it lies past the
            // match, since that is where the parser got stuck. Otherwise the
            // input simply continues past the rule.
            Some(end) if ctx.furthest > end => Err(ParseError {
                position: ctx.furthest,
                expected: ctx.expected,
            }),
            Some(end) => Err(ParseError {
                position: end,
                expected: Expected::EndOfInput,
            }),
            None => Err(ParseError {
                position: ctx.furthest,
                expected: ctx.expected,
            }),
        }
    }

    /// Returns `true` if rule `rule` matches all of `input`. Builds no tree,
    /// so it does not allocate.
    pub fn recognise(&self, rule: usize, input: &[u8]) -> bool {
        let mut ctx = Ctx {
            grammar: self,
            input,
            build: false,
            furthest: 0,
            expected: Expected::EndOfInput,
        };
        let mut kids = Vec::new();
        eval_call(&mut ctx, rule, 0, 0, &mut kids) == Some(input.len())
    }
}

/// Matches rule `index` at `pos`. Returns the end position on success.
fn eval_call(
    ctx: &mut Ctx<'_, '_>,
    index: usize,
    pos: usize,
    depth: usize,
    kids: &mut Vec<Tree>,
) -> Option<usize> {
    if depth > MAX_DEPTH {
        ctx.fail(pos, Expected::Recursion);
        return None;
    }
    let body = ctx.grammar.rules()[index].body;
    let mut sub = Vec::new();
    let end = eval(ctx, &body, pos, depth + 1, &mut sub)?;
    if ctx.build {
        kids.push(Tree {
            rule: index,
            start: pos,
            end,
            children: sub,
        });
    }
    Some(end)
}

/// Matches expression `e` at `pos`. On failure, any children pushed are removed again.
fn eval(
    ctx: &mut Ctx<'_, '_>,
    e: &Expr,
    pos: usize,
    depth: usize,
    kids: &mut Vec<Tree>,
) -> Option<usize> {
    let mark = kids.len();
    let result = match *e {
        Expr::Lit(bytes) => {
            if ctx.input[pos..].starts_with(bytes) {
                Some(pos + bytes.len())
            } else {
                ctx.fail(pos, Expected::Literal(bytes));
                None
            }
        }
        Expr::Range(lo, hi) => match ctx.input.get(pos) {
            Some(&b) if (lo..=hi).contains(&b) => Some(pos + 1),
            _ => {
                ctx.fail(pos, Expected::Range(lo, hi));
                None
            }
        },
        Expr::Seq(items) => {
            let mut p = pos;
            let mut ok = true;
            for item in items {
                match eval(ctx, item, p, depth, kids) {
                    Some(np) => p = np,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok { Some(p) } else { None }
        }
        Expr::Choice(alts) => alts.iter().find_map(|alt| eval(ctx, alt, pos, depth, kids)),
        Expr::Repeat(inner) => {
            let mut p = pos;
            loop {
                match eval(ctx, inner, p, depth, kids) {
                    Some(np) if np > p => p = np,
                    _ => break,
                }
            }
            Some(p)
        }
        Expr::Optional(inner) => Some(eval(ctx, inner, pos, depth, kids).unwrap_or(pos)),
        Expr::Call(index) => eval_call(ctx, index, pos, depth, kids),
    };
    if result.is_none() {
        kids.truncate(mark);
    }
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::grammar::Rule;

    const DIGIT: Expr = Expr::Range(b'0', b'9');
    const WORDS: Grammar = Grammar::new(&[
        Rule {
            name: "list",
            body: Expr::Seq(&[
                Expr::Call(1),
                Expr::Repeat(&Expr::Seq(&[Expr::Lit(b","), Expr::Call(1)])),
            ]),
        },
        Rule {
            name: "digits",
            body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
        },
    ]);

    #[test]
    fn matches_and_builds_tree() {
        let t = WORDS.parse(0, b"12,3,456").unwrap();
        assert_eq!((t.rule, t.start, t.end), (0, 0, 8));
        assert_eq!(t.children.len(), 3);
        assert_eq!(t.children[2].start, 5);
        assert_eq!(t.children[2].end, 8);
    }

    #[test]
    fn reports_furthest_failure() {
        let e = WORDS.parse(0, b"12,x").unwrap_err();
        assert_eq!(e.position, 3);
        assert_eq!(e.expected, Expected::Range(b'0', b'9'));
    }

    #[test]
    fn trailing_input_is_an_error() {
        let e = WORDS.parse(1, b"12x").unwrap_err();
        assert_eq!(e.expected, Expected::EndOfInput);
        assert_eq!(e.position, 2);
    }

    #[test]
    fn recognise_does_not_build_a_tree() {
        assert!(WORDS.recognise(0, b"1,2"));
        assert!(!WORDS.recognise(0, b"1,"));
    }

    const LEFT: Grammar = Grammar::new(&[Rule {
        name: "loop",
        body: Expr::Seq(&[Expr::Call(0), Expr::Lit(b"x")]),
    }]);

    #[test]
    fn left_recursion_is_an_error_not_an_overflow() {
        assert!(LEFT.parse(0, b"x").is_err());
        assert!(!LEFT.recognise(0, b"x"));
    }
}

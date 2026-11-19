//! Random grammars checked against the interpreter, plus a left-recursion case
//! whose result depends on the call depth.
//!
//! The random grammars only call later rules, so the interpreter (which has no
//! memo) stays fast. Left recursion that branches is exponential for the
//! interpreter, so it cannot serve as the reference there.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_dsl::{Expr, Grammar, Memo, PackratError, Rule};

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

fn leak<T>(v: Vec<T>) -> &'static [T] {
    Box::leak(v.into_boxed_slice())
}

fn leak_expr(e: Expr) -> &'static Expr {
    Box::leak(Box::new(e))
}

struct Gen {
    rng: XorShift,
    rules: usize,
    /// Rule currently being generated.
    current: usize,
}

impl Gen {
    fn leaf(&mut self) -> Expr {
        match self.rng.below(6) {
            0 => Expr::Lit(b"a"),
            1 => Expr::Lit(b"b"),
            2 => Expr::Lit(b"ab"),
            3 => Expr::Lit(b""),
            4 => Expr::Range(b'a', b'b'),
            _ => self.call(),
        }
    }

    fn call(&mut self) -> Expr {
        let i = self.current;
        if i + 1 < self.rules {
            Expr::Call(i + 1 + self.rng.below(self.rules - i - 1))
        } else {
            Expr::Lit(b"b")
        }
    }

    fn expr(&mut self, depth: usize) -> Expr {
        if depth == 0 || self.rng.below(5) < 2 {
            return self.leaf();
        }
        match self.rng.below(4) {
            0 => {
                let n = 2 + self.rng.below(2);
                Expr::Seq(leak((0..n).map(|_| self.expr(depth - 1)).collect()))
            }
            1 => {
                let n = 2 + self.rng.below(2);
                Expr::Choice(leak((0..n).map(|_| self.expr(depth - 1)).collect()))
            }
            2 => Expr::Repeat(leak_expr(self.expr(depth - 1))),
            _ => Expr::Optional(leak_expr(self.expr(depth - 1))),
        }
    }
}

fn random_grammar(seed: u64) -> Grammar {
    let mut g = Gen {
        rng: XorShift(seed | 1),
        rules: 1 + (seed as usize % 4),
        current: 0,
    };
    let mut rules = Vec::new();
    for i in 0..g.rules {
        g.current = i;
        let body = g.expr(3);
        rules.push(Rule {
            name: Box::leak(format!("r{i}").into_boxed_str()),
            body,
        });
    }
    Grammar::new(leak(rules))
}

fn random_input(rng: &mut XorShift) -> Vec<u8> {
    let len = rng.below(10);
    (0..len)
        .map(|_| if rng.below(4) == 0 { b'b' } else { b'a' })
        .collect()
}

#[test]
fn packrat_matches_interpreter_on_random_grammars() {
    let mut rng = XorShift(0xDEAD_BEEF_CAFE_F00D);
    let mut memo = Box::new(Memo::<4096>::new());
    let mut accepted = 0;
    for seed in 1..=2500u64 {
        let grammar = random_grammar(seed);
        for rule in 0..grammar.rules().len() {
            for _ in 0..6 {
                let input = random_input(&mut rng);
                memo.clear();
                let interpreted = grammar.parse(rule, &input);
                let packrat = grammar.parse_packrat(rule, &input, &mut memo);
                match (&interpreted, &packrat) {
                    (Ok(a), Ok(b)) => {
                        accepted += 1;
                        assert_eq!(a, b, "seed {seed} rule {rule} input {input:?}");
                    }
                    (Err(a), Err(PackratError::Parse(b))) => {
                        assert_eq!(a, b, "seed {seed} rule {rule} input {input:?}");
                    }
                    _ => panic!(
                        "seed {seed} rule {rule} input {input:?}: {interpreted:?} vs {packrat:?}"
                    ),
                }
                memo.clear();
                assert_eq!(
                    grammar.recognise(rule, &input),
                    grammar.recognise_packrat(rule, &input, &mut memo).unwrap(),
                    "seed {seed} rule {rule} input {input:?}"
                );
            }
        }
    }
    assert!(accepted > 500, "only {accepted} accepted parses");
}

/// `S = X "!" / chain`, where the chain reaches `X` at a depth near 100.
/// `X` is left recursive and succeeds at the depth limit, so the end it reaches
/// depends on the depth it starts at. A memo entry from the shallow call must
/// not be reused for the deep one.
#[test]
fn depth_dependent_results_are_not_reused() {
    const CHAIN: usize = 98;
    let mut rules = vec![
        Rule {
            name: "start",
            body: Expr::Choice(leak(vec![
                Expr::Seq(leak(vec![Expr::Call(1), Expr::Lit(b"!")])),
                Expr::Call(2),
            ])),
        },
        Rule {
            name: "x",
            body: Expr::Choice(leak(vec![
                Expr::Seq(leak(vec![Expr::Call(1), Expr::Lit(b"x")])),
                Expr::Lit(b"y"),
            ])),
        },
    ];
    for k in 0..CHAIN {
        rules.push(Rule {
            name: "chain",
            body: Expr::Call(3 + k),
        });
    }
    rules.push(Rule {
        name: "tail",
        body: Expr::Seq(leak(vec![Expr::Call(1), Expr::Lit(b"z")])),
    });
    let grammar = Grammar::new(leak(rules));

    // Read from depth 100, X ends after "y" and 28 x's, so "z" follows it.
    let mut input = vec![b'y'];
    input.extend(std::iter::repeat_n(b'x', 28));
    input.push(b'z');
    let mut memo = Memo::<1024>::new();
    let interpreted = grammar.parse(0, &input);
    let packrat = grammar.parse_packrat(0, &input, &mut memo);
    assert!(interpreted.is_ok(), "the chain should accept the input");
    assert_eq!(packrat.as_ref().ok(), interpreted.as_ref().ok());
    memo.clear();
    assert_eq!(grammar.recognise_packrat(0, &input, &mut memo), Ok(true));
}

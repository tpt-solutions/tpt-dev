//! Randomised cross-check against an independent oracle.
//!
//! The oracle converts each random formula to disjunctive normal form, then
//! decides each conjunction exactly with Fourier-Motzkin elimination. Every
//! `Sat` answer is also checked by evaluating the formula at the model.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_solver::{Arena, Formula, Linear, Rational, Solver, Status, Var};

/// A tiny xorshift generator, so the test is deterministic.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

#[derive(Clone, Copy, Debug)]
enum Rel {
    Le,
    Lt,
    Ge,
    Gt,
    Eq,
}

#[derive(Clone, Debug)]
enum Ast {
    Atom { coef: [i64; 2], rel: Rel, rhs: i64 },
    Not(Box<Ast>),
    And(Vec<Ast>),
    Or(Vec<Ast>),
}

fn random_ast(rng: &mut Rng, depth: u32) -> Ast {
    if depth == 0 || rng.range(0, 3) == 0 {
        let rel = match rng.range(0, 4) {
            0 => Rel::Le,
            1 => Rel::Lt,
            2 => Rel::Ge,
            3 => Rel::Gt,
            _ => Rel::Eq,
        };
        return Ast::Atom {
            coef: [rng.range(-3, 3), rng.range(-3, 3)],
            rel,
            rhs: rng.range(-5, 5),
        };
    }
    let n = rng.range(2, 3) as usize;
    let kids: Vec<Ast> = (0..n).map(|_| random_ast(rng, depth - 1)).collect();
    match rng.range(0, 2) {
        0 => Ast::And(kids),
        1 => Ast::Or(kids),
        _ => Ast::Not(Box::new(random_ast(rng, depth - 1))),
    }
}

fn build(arena: &mut Arena, vars: [Var; 2], ast: &Ast) -> Formula {
    match ast {
        Ast::Atom { coef, rel, rhs } => {
            let lhs = Linear::var(vars[0])
                .scale(Rational::from_int(coef[0] as i128))
                .unwrap()
                .checked_add(
                    &Linear::var(vars[1])
                        .scale(Rational::from_int(coef[1] as i128))
                        .unwrap(),
                )
                .unwrap();
            let c = Linear::constant(Rational::from_int(*rhs as i128));
            match rel {
                Rel::Le => arena.le(&lhs, &c).unwrap(),
                Rel::Lt => arena.lt(&lhs, &c).unwrap(),
                Rel::Ge => arena.ge(&lhs, &c).unwrap(),
                Rel::Gt => arena.gt(&lhs, &c).unwrap(),
                Rel::Eq => arena.eq(&lhs, &c).unwrap(),
            }
        }
        Ast::Not(g) => {
            let f = build(arena, vars, g);
            arena.not(f)
        }
        Ast::And(gs) => {
            let fs = gs.iter().map(|g| build(arena, vars, g)).collect();
            arena.and(fs)
        }
        Ast::Or(gs) => {
            let fs = gs.iter().map(|g| build(arena, vars, g)).collect();
            arena.or(fs)
        }
    }
}

/// Evaluates the formula at a model.
fn eval(ast: &Ast, x: [Rational; 2]) -> bool {
    match ast {
        Ast::Atom { coef, rel, rhs } => {
            let lhs = x[0]
                .checked_mul(Rational::from_int(coef[0] as i128))
                .unwrap()
                .checked_add(
                    x[1].checked_mul(Rational::from_int(coef[1] as i128))
                        .unwrap(),
                )
                .unwrap();
            let c = Rational::from_int(*rhs as i128);
            match rel {
                Rel::Le => lhs <= c,
                Rel::Lt => lhs < c,
                Rel::Ge => lhs >= c,
                Rel::Gt => lhs > c,
                Rel::Eq => lhs == c,
            }
        }
        Ast::Not(g) => !eval(g, x),
        Ast::And(gs) => gs.iter().all(|g| eval(g, x)),
        Ast::Or(gs) => gs.iter().any(|g| eval(g, x)),
    }
}

/// A linear constraint `coef . x <= rhs` (or `<` when strict), over two variables.
#[derive(Clone, Debug)]
struct Constraint {
    coef: [Rational; 2],
    rhs: Rational,
    strict: bool,
}

fn r(n: i64) -> Rational {
    Rational::from_int(n as i128)
}

/// Disjunctive normal form: a list of conjunctions of constraints.
fn dnf(ast: &Ast, positive: bool) -> Vec<Vec<Constraint>> {
    match ast {
        Ast::Atom { coef, rel, rhs } => {
            let a = [r(coef[0]), r(coef[1])];
            let neg = |a: [Rational; 2]| [a[0].checked_neg().unwrap(), a[1].checked_neg().unwrap()];
            // Each relation as one or more constraints: `a.x <= c` (strict if flagged) or `-a.x <= -c`.
            let le = |a: [Rational; 2], c: i64, strict: bool| Constraint {
                coef: a,
                rhs: r(c),
                strict,
            };
            let ge = |a: [Rational; 2], c: i64, strict: bool| Constraint {
                coef: neg(a),
                rhs: r(-c),
                strict,
            };
            // Effective relation after applying negation.
            let rel = if positive {
                *rel
            } else {
                match rel {
                    Rel::Le => Rel::Gt,
                    Rel::Lt => Rel::Ge,
                    Rel::Ge => Rel::Lt,
                    Rel::Gt => Rel::Le,
                    Rel::Eq => return vec![vec![le(a, *rhs, true)], vec![ge(a, *rhs, true)]],
                }
            };
            let c = match rel {
                Rel::Le => vec![le(a, *rhs, false)],
                Rel::Lt => vec![le(a, *rhs, true)],
                Rel::Ge => vec![ge(a, *rhs, false)],
                Rel::Gt => vec![ge(a, *rhs, true)],
                Rel::Eq => vec![le(a, *rhs, false), ge(a, *rhs, false)],
            };
            vec![c]
        }
        Ast::Not(g) => dnf(g, !positive),
        Ast::And(gs) if positive => product(gs.iter().map(|g| dnf(g, true)).collect()),
        Ast::Or(gs) if !positive => product(gs.iter().map(|g| dnf(g, false)).collect()),
        Ast::And(gs) => union(gs.iter().map(|g| dnf(g, false)).collect()),
        Ast::Or(gs) => union(gs.iter().map(|g| dnf(g, true)).collect()),
    }
}

fn union(parts: Vec<Vec<Vec<Constraint>>>) -> Vec<Vec<Constraint>> {
    parts.into_iter().flatten().collect()
}

fn product(parts: Vec<Vec<Vec<Constraint>>>) -> Vec<Vec<Constraint>> {
    let mut acc: Vec<Vec<Constraint>> = vec![Vec::new()];
    for part in parts {
        let mut next = Vec::new();
        for a in &acc {
            for b in &part {
                let mut c = a.clone();
                c.extend(b.iter().cloned());
                next.push(c);
            }
        }
        acc = next;
    }
    acc
}

/// Exact satisfiability of a conjunction of two-variable constraints, by
/// eliminating `x0` and then `x1` (Fourier-Motzkin).
fn fm_sat(mut cs: Vec<Constraint>) -> bool {
    for var in 0..2 {
        let mut next = Vec::new();
        let (mut pos, mut neg) = (Vec::new(), Vec::new());
        for c in cs {
            let k = c.coef[var];
            if k.is_zero() {
                next.push(c);
            } else if k.is_negative() {
                neg.push(c);
            } else {
                pos.push(c);
            }
        }
        for p in &pos {
            for n in &neg {
                // p: a x + ... <= b, with a > 0; n: -d x + ... <= e, with d > 0.
                // Multiply p by d and n by a, then add to cancel x.
                let a = p.coef[var];
                let d = n.coef[var].checked_neg().unwrap();
                let mut coef = [Rational::ZERO; 2];
                for (i, slot) in coef.iter_mut().enumerate() {
                    *slot = p.coef[i]
                        .checked_mul(d)
                        .unwrap()
                        .checked_add(n.coef[i].checked_mul(a).unwrap())
                        .unwrap();
                }
                next.push(Constraint {
                    coef,
                    rhs: p
                        .rhs
                        .checked_mul(d)
                        .unwrap()
                        .checked_add(n.rhs.checked_mul(a).unwrap())
                        .unwrap(),
                    strict: p.strict || n.strict,
                });
            }
        }
        cs = next;
    }
    // Only constant constraints remain: 0 <= rhs (or 0 < rhs).
    cs.iter().all(|c| {
        if c.strict {
            !c.rhs.is_negative() && !c.rhs.is_zero()
        } else {
            !c.rhs.is_negative()
        }
    })
}

fn oracle_sat(ast: &Ast) -> bool {
    dnf(ast, true).into_iter().any(fm_sat)
}

#[test]
fn random_formulas_agree_with_oracle() {
    let seed: u64 = std::env::var("XC_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    let cases: usize = std::env::var("XC_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4000);
    let depth: u32 = std::env::var("XC_DEPTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let mut rng = Rng(seed);
    let mut sat_count = 0;
    let mut unsat_count = 0;
    for case in 0..cases {
        let ast = random_ast(&mut rng, depth);
        let mut solver = Solver::new();
        let x = solver.declare("x");
        let y = solver.declare("y");
        let f = build(solver.arena_mut(), [x, y], &ast);
        solver.assert(f);
        let got = solver.check().unwrap();
        let want = oracle_sat(&ast);
        match got {
            Status::Sat => {
                sat_count += 1;
                let model = [solver.value(x).unwrap(), solver.value(y).unwrap()];
                if !eval(&ast, model) {
                    panic!("case {case}: model {model:?} does not satisfy {ast:?}");
                }
            }
            Status::Unsat => unsat_count += 1,
        }
        assert_eq!(got == Status::Sat, want, "case {case}: {ast:?}");
    }
    // The generator should produce both kinds, or the test proves little.
    if depth >= 3 {
        assert!(
            sat_count > 100 && unsat_count > 100,
            "sat {sat_count}, unsat {unsat_count}"
        );
    }
}

//! Shared random-formula generator for the integration tests. Each test
//! binary uses a different subset of these items.
#![allow(dead_code, clippy::unwrap_used, clippy::panic)]

// The unit-test build is `no_std`, so it needs these from `std`.
#[cfg(test)]
use std::{boxed::Box, vec::Vec};

use tpt_dev_solver::{Arena, FixedArena, Formula, Linear, Rational, Var};

/// A tiny xorshift generator, so the tests are deterministic.
pub(crate) struct Rng(pub u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(crate) fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Rel {
    Le,
    Lt,
    Ge,
    Gt,
    Eq,
}

/// A Boolean formula over linear atoms in two variables.
#[derive(Clone, Debug)]
pub(crate) enum Ast {
    Atom { coef: [i64; 2], rel: Rel, rhs: i64 },
    Not(Box<Ast>),
    And(Vec<Ast>),
    Or(Vec<Ast>),
}

pub(crate) fn random_atom(rng: &mut Rng) -> Ast {
    let rel = match rng.range(0, 4) {
        0 => Rel::Le,
        1 => Rel::Lt,
        2 => Rel::Ge,
        3 => Rel::Gt,
        _ => Rel::Eq,
    };
    Ast::Atom {
        coef: [rng.range(-3, 3), rng.range(-3, 3)],
        rel,
        rhs: rng.range(-5, 5),
    }
}

pub(crate) fn random_ast(rng: &mut Rng, depth: u32) -> Ast {
    if depth == 0 || rng.range(0, 3) == 0 {
        return random_atom(rng);
    }
    let n = rng.range(2, 3) as usize;
    let kids: Vec<Ast> = (0..n).map(|_| random_ast(rng, depth - 1)).collect();
    match rng.range(0, 2) {
        0 => Ast::And(kids),
        1 => Ast::Or(kids),
        _ => Ast::Not(Box::new(random_ast(rng, depth - 1))),
    }
}

/// Builds `ast` in an [`Arena`] with the linear builders.
pub(crate) fn build_arena(arena: &mut Arena, vars: [Var; 2], ast: &Ast) -> Formula {
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
        Ast::Not(a) => {
            let f = build_arena(arena, vars, a);
            arena.not(f)
        }
        Ast::And(kids) => {
            let fs = kids.iter().map(|k| build_arena(arena, vars, k)).collect();
            arena.and(fs)
        }
        Ast::Or(kids) => {
            let fs = kids.iter().map(|k| build_arena(arena, vars, k)).collect();
            arena.or(fs)
        }
    }
}

/// Builds one atom `sum(coef * vars) rel rhs` in a fixed arena.
fn fixed_cmp(
    arena: &mut FixedArena<2, 512, 1024, 1024>,
    vars: [Var; 2],
    coef: [i64; 2],
    strict: bool,
    rhs: i64,
) -> Formula {
    let mut terms = Vec::new();
    for i in 0..2 {
        if coef[i] != 0 {
            terms.push((vars[i], Rational::from_int(coef[i] as i128)));
        }
    }
    arena
        .atom(&terms, Rational::from_int(rhs as i128), strict)
        .unwrap()
}

/// Builds `ast` in a fixed-capacity arena. Panics if the capacity is too small.
pub(crate) fn build_fixed(
    arena: &mut FixedArena<2, 512, 1024, 1024>,
    vars: [Var; 2],
    ast: &Ast,
) -> Formula {
    match ast {
        Ast::Atom { coef, rel, rhs } => {
            let neg = [-coef[0], -coef[1]];
            match rel {
                Rel::Le => fixed_cmp(arena, vars, *coef, false, *rhs),
                Rel::Lt => fixed_cmp(arena, vars, *coef, true, *rhs),
                Rel::Ge => fixed_cmp(arena, vars, neg, false, -rhs),
                Rel::Gt => fixed_cmp(arena, vars, neg, true, -rhs),
                Rel::Eq => {
                    let a = fixed_cmp(arena, vars, *coef, false, *rhs);
                    let b = fixed_cmp(arena, vars, neg, false, -rhs);
                    arena.and(&[a, b]).unwrap()
                }
            }
        }
        Ast::Not(a) => {
            let f = build_fixed(arena, vars, a);
            arena.not(f).unwrap()
        }
        Ast::And(kids) => {
            let fs: Vec<Formula> = kids.iter().map(|k| build_fixed(arena, vars, k)).collect();
            arena.and(&fs).unwrap()
        }
        Ast::Or(kids) => {
            let fs: Vec<Formula> = kids.iter().map(|k| build_fixed(arena, vars, k)).collect();
            arena.or(&fs).unwrap()
        }
    }
}

/// Evaluates `ast` at the point `v`.
pub(crate) fn eval(ast: &Ast, v: [Rational; 2]) -> bool {
    match ast {
        Ast::Atom { coef, rel, rhs } => {
            let lhs = v[0]
                .checked_mul(Rational::from_int(coef[0] as i128))
                .unwrap()
                .checked_add(
                    v[1].checked_mul(Rational::from_int(coef[1] as i128))
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
        Ast::Not(a) => !eval(a, v),
        Ast::And(kids) => kids.iter().all(|k| eval(k, v)),
        Ast::Or(kids) => kids.iter().any(|k| eval(k, v)),
    }
}

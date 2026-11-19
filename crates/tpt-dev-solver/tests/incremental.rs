//! Incremental check with push and pop must agree with a from-scratch solve
//! of the same assertions, over randomized sequences.

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use common::*;
use tpt_dev_solver::{Solver, Status};

/// Solves `asts` in a fresh solver.
fn from_scratch(asts: &[Ast]) -> Status {
    let mut s = Solver::new();
    let x = s.declare("x");
    let y = s.declare("y");
    for a in asts {
        let f = build_arena(s.arena_mut(), [x, y], a);
        s.assert(f);
    }
    s.check().unwrap()
}

#[test]
fn push_pop_matches_from_scratch_solve() {
    let (mut sat, mut unsat) = (0, 0);
    for seed in 1..=80u64 {
        let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
        let mut s = Solver::new();
        let x = s.declare("x");
        let y = s.declare("y");
        // frames[0] is the base scope; each later frame matches one push.
        let mut frames: Vec<Vec<Ast>> = vec![Vec::new()];
        for _ in 0..50 {
            match rng.range(0, 9) {
                0..=3 => {
                    let ast = random_ast(&mut rng, 2);
                    let f = build_arena(s.arena_mut(), [x, y], &ast);
                    s.assert(f);
                    frames.last_mut().unwrap().push(ast);
                }
                4 => {
                    s.push();
                    frames.push(Vec::new());
                }
                5 => {
                    if frames.len() > 1 {
                        assert!(s.pop());
                        frames.pop();
                    }
                }
                _ => {
                    let all: Vec<Ast> = frames.iter().flatten().cloned().collect();
                    let expect = from_scratch(&all);
                    let got = s.check().unwrap();
                    assert_eq!(got, expect, "seed {seed}, assertions {all:?}");
                    if got == Status::Sat {
                        let vals = [s.value(x).unwrap(), s.value(y).unwrap()];
                        for a in &all {
                            assert!(eval(a, vals), "seed {seed}: model fails {a:?}");
                        }
                        sat += 1;
                    } else {
                        unsat += 1;
                    }
                }
            }
        }
    }
    assert!(sat > 50 && unsat > 50, "sat {sat}, unsat {unsat}");
}

#[test]
fn learned_clauses_do_not_leak_across_pop() {
    let mut s = Solver::new();
    let x = s.declare("x");
    let y = s.declare("y");
    let base = Ast::Atom {
        coef: [0, 1],
        rel: Rel::Ge,
        rhs: 2,
    };
    let f = build_arena(s.arena_mut(), [x, y], &base);
    s.assert(f);
    s.push();
    // x <= 0 and x >= 1 conflict.
    let a = build_arena(
        s.arena_mut(),
        [x, y],
        &Ast::Atom {
            coef: [1, 0],
            rel: Rel::Le,
            rhs: 0,
        },
    );
    let b = build_arena(
        s.arena_mut(),
        [x, y],
        &Ast::Atom {
            coef: [1, 0],
            rel: Rel::Ge,
            rhs: 1,
        },
    );
    s.assert(a);
    s.assert(b);
    assert_eq!(s.check().unwrap(), Status::Unsat);
    assert!(s.pop());
    // Clauses learned under the pushed scope must not affect the base.
    assert_eq!(s.check().unwrap(), from_scratch(&[base]));
    assert_eq!(s.check().unwrap(), Status::Sat);
}

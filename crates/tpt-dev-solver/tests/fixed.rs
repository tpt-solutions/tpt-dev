//! The fixed-capacity arena must give the same answers as the heap-backed one.

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use common::*;
use tpt_dev_solver::{Arena, Budget, FixedArena, Rational, Solver, Status, Unknown};

type Fixed = FixedArena<2, 512, 1024, 1024>;

#[test]
fn fixed_arena_agrees_with_arena_solver() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let (mut sat, mut unsat) = (0, 0);
    for _ in 0..3000 {
        let depth = rng.range(0, 3) as u32;
        let ast = random_ast(&mut rng, depth);

        let mut solver = Solver::new();
        let x = solver.declare("x");
        let y = solver.declare("y");
        let f = build_arena(solver.arena_mut(), [x, y], &ast);
        solver.assert(f);
        let expect = solver.check().unwrap();

        let mut fixed: Fixed = FixedArena::new();
        let fx = fixed.declare("x").unwrap();
        let fy = fixed.declare("y").unwrap();
        let g = build_fixed(&mut fixed, [fx, fy], &ast);
        let mut model = [Rational::ZERO; 2];
        let got = fixed
            .check_model(&[g], &Budget::UNLIMITED, &mut model)
            .unwrap();

        assert_eq!(got, expect, "ast {ast:?}");
        if got == Status::Sat {
            assert!(eval(&ast, model), "model does not satisfy {ast:?}");
            sat += 1;
        } else {
            unsat += 1;
        }
    }
    assert!(sat > 100 && unsat > 100, "sat {sat}, unsat {unsat}");
}

#[test]
fn fixed_arena_refuses_when_full() {
    let mut a: FixedArena<1, 2, 2, 2> = FixedArena::new();
    let _x = a.declare("x").unwrap();
    assert!(a.declare("y").is_none(), "variable pool is full");

    let t = a.constant(true).unwrap();
    let _ = a.not(t).unwrap();
    assert_eq!(a.num_nodes(), 2);
    assert!(a.constant(false).is_none(), "node pool is full");

    let mut b: FixedArena<2, 8, 1, 8> = FixedArena::new();
    let p = b.declare("p").unwrap();
    let q = b.declare("q").unwrap();
    let one = Rational::ONE;
    // Two terms do not fit in a term pool of one.
    assert!(b.atom(&[(p, one), (q, one)], one, false).is_none());
    // Terms must be sorted by variable, with nonzero coefficients.
    assert!(b.atom(&[(q, one), (p, one)], one, false).is_none());
    assert!(b.atom(&[(p, Rational::ZERO)], one, false).is_none());
    assert!(b.atom(&[(p, one)], one, false).is_some());
    assert_eq!(b.var_name(p), "p");
}

#[test]
fn fixed_arena_budget_applies() {
    let mut a: Fixed = FixedArena::new();
    let x = a.declare("x").unwrap();
    let y = a.declare("y").unwrap();
    let f = a.atom(&[(x, Rational::ONE)], Rational::ONE, false).unwrap();
    let g = a.atom(&[(y, Rational::ONE)], Rational::ONE, false).unwrap();
    let h = a.or(&[f, g]).unwrap();
    let tight = Budget {
        max_formula_nodes: 1,
        ..Budget::UNLIMITED
    };
    assert_eq!(a.check(&[h], &tight), Err(Unknown));
    assert_eq!(a.check(&[h], &Budget::UNLIMITED), Ok(Status::Sat));
}

#[test]
fn heap_arena_unaffected() {
    let mut arena = Arena::new();
    let x = arena.declare("x");
    let _ = arena.constant(true);
    assert_eq!(arena.num_vars(), 1);
    assert_eq!(arena.var_name(x), "x");
}

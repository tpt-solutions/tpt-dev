//! Farkas certificates: every unsat conjunction of linear literals gets one,
//! it checks, and tampered certificates are rejected.

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use common::*;
use tpt_dev_solver::{Farkas, Rational, Solver, Status, check_farkas};

fn atom(coef: [i64; 2], rel: Rel, rhs: i64) -> Ast {
    Ast::Atom { coef, rel, rhs }
}

#[test]
fn certificates_check_on_random_conjunctions() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let (mut unsat, mut sat) = (0, 0);
    for _ in 0..4000 {
        let n = rng.range(1, 6) as usize;
        let atoms: Vec<Ast> = (0..n).map(|_| random_atom(&mut rng)).collect();
        let mut s = Solver::new();
        let x = s.declare("x");
        let y = s.declare("y");
        for a in &atoms {
            let f = build_arena(s.arena_mut(), [x, y], a);
            s.assert(f);
        }
        let status = s.check().unwrap();
        let cert = s.unsat_certificate().unwrap();
        match status {
            Status::Unsat => {
                let c = cert.unwrap_or_else(|| panic!("no certificate for {atoms:?}"));
                assert!(
                    s.check_certificate(&c),
                    "certificate rejected for {atoms:?}"
                );
                assert!(check_farkas(s.arena(), s.assertions(), &c));
                unsat += 1;
            }
            Status::Sat => {
                assert!(cert.is_none(), "certificate for satisfiable {atoms:?}");
                sat += 1;
            }
        }
    }
    assert!(unsat > 200 && sat > 200, "unsat {unsat}, sat {sat}");
}

#[test]
fn tampered_certificates_are_rejected() {
    // x <= 1 (literal 0) and x >= 2 (literal 1).
    let mut s = Solver::new();
    let x = s.declare("x");
    let y = s.declare("y");
    for a in [atom([1, 0], Rel::Le, 1), atom([1, 0], Rel::Ge, 2)] {
        let f = build_arena(s.arena_mut(), [x, y], &a);
        s.assert(f);
    }
    let one = Rational::ONE;
    let minus_one = Rational::from_int(-1);
    let two = Rational::from_int(2);
    let cert = s.unsat_certificate().unwrap().unwrap();
    assert!(s.check_certificate(&cert));
    // Weights that do not cancel the variable.
    assert!(!s.check_certificate(&Farkas::new(vec![(0, one), (1, two)])));
    // A single literal is satisfiable.
    assert!(!s.check_certificate(&Farkas::new(vec![(0, one)])));
    // Nonpositive weight, repeated index, out-of-range index, and empty.
    assert!(!s.check_certificate(&Farkas::new(vec![(0, minus_one), (1, minus_one)])));
    assert!(!s.check_certificate(&Farkas::new(vec![(0, one), (0, one), (1, one)])));
    assert!(!s.check_certificate(&Farkas::new(vec![(0, one), (9, one)])));
    assert!(!s.check_certificate(&Farkas::new(Vec::new())));
}

#[test]
fn strict_bounds_need_the_delta_part() {
    // x < 1 and x >= 1 conflict.
    let mut s = Solver::new();
    let x = s.declare("x");
    let y = s.declare("y");
    for a in [atom([1, 0], Rel::Lt, 1), atom([1, 0], Rel::Ge, 1)] {
        let f = build_arena(s.arena_mut(), [x, y], &a);
        s.assert(f);
    }
    assert_eq!(s.check().unwrap(), Status::Unsat);
    let cert = s.unsat_certificate().unwrap().unwrap();
    assert!(s.check_certificate(&cert));

    // x <= 1 and not(x <= 1) conflict through a negated atom.
    let mut t = Solver::new();
    let x = t.declare("x");
    let y = t.declare("y");
    let a = build_arena(t.arena_mut(), [x, y], &atom([1, 0], Rel::Le, 1));
    let na = t.arena_mut().not(a);
    t.assert(a);
    t.assert(na);
    let cert = t.unsat_certificate().unwrap().unwrap();
    assert!(t.check_certificate(&cert));
}

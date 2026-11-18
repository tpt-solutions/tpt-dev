//! Solver-backed generation, and invariant inference checked by the solver.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_solver::{Linear, Rational, Solver};
use tpt_dev_test::{
    Candidate, ConstrainedGen, Generator, Rng, Seed, SetupError, Verdict, check, infer, prove,
};

fn int(n: i64) -> Linear {
    Linear::constant(Rational::from_int(i128::from(n)))
}

/// x + y = 10, x >= 0, y >= 0.
fn simplex_constraint(
    arena: &mut tpt_dev_solver::Arena,
    v: &[tpt_dev_solver::Var],
) -> Option<tpt_dev_solver::Formula> {
    let sum = Linear::var(v[0]).checked_add(&Linear::var(v[1]))?;
    let eq = arena.eq(&sum, &int(10))?;
    let x_ok = arena.ge(&Linear::var(v[0]), &int(0))?;
    let y_ok = arena.ge(&Linear::var(v[1]), &int(0))?;
    Some(arena.and(vec![eq, x_ok, y_ok]))
}

#[test]
fn samples_satisfy_the_constraint_and_vary() {
    let generator = ConstrainedGen::new(&["x", "y"], simplex_constraint).unwrap();
    let mut rng = Rng::new(Seed(11));
    let mut distinct = std::collections::BTreeSet::new();
    for _ in 0..200 {
        let s = generator.generate(&mut rng, 10);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].checked_add(s[1]).unwrap(), Rational::from_int(10));
        assert!(s[0] >= Rational::ZERO && s[1] >= Rational::ZERO);
        distinct.insert(s[0]);
    }
    assert!(
        distinct.len() > 5,
        "samples should vary, got {} distinct",
        distinct.len()
    );
}

#[test]
fn unsatisfiable_constraint_is_a_setup_error() {
    let r = ConstrainedGen::new(&["x"], |arena, v| {
        let a = arena.gt(&Linear::var(v[0]), &int(1))?;
        let b = arena.lt(&Linear::var(v[0]), &int(0))?;
        Some(arena.and(vec![a, b]))
    });
    assert!(matches!(r, Err(SetupError::Unsatisfiable)));
}

#[test]
fn property_over_constrained_inputs_holds() {
    let generator = ConstrainedGen::new(&["x", "y"], simplex_constraint).unwrap();
    let config = tpt_dev_test::Config {
        cases: 100,
        ..tpt_dev_test::Config::default()
    };
    // Every sample has x*y <= 25, since x + y = 10 gives x*y = x(10-x) <= 25 by AM-GM.
    let result = check(&config, &generator, |s: &Vec<Rational>| {
        s[0].checked_mul(s[1]).unwrap() <= Rational::from_int(25)
    });
    assert!(result.is_ok());
}

#[test]
fn inferred_invariants_are_proved_or_refuted_with_checked_witnesses() {
    // Samples come from a narrower set (3 <= x <= 7) than the precondition
    // (x, y >= 0), so sample-derived bounds need not hold for the precondition.
    let narrow = ConstrainedGen::new(&["x", "y"], |arena, v| {
        let base = simplex_constraint(arena, v)?;
        let lo = arena.ge(&Linear::var(v[0]), &int(3))?;
        let hi = arena.le(&Linear::var(v[0]), &int(7))?;
        Some(arena.and(vec![base, lo, hi]))
    })
    .unwrap();
    let mut rng = Rng::new(Seed(23));
    let samples: Vec<Vec<Rational>> = (0..200).map(|_| narrow.generate(&mut rng, 10)).collect();
    let candidates = infer(&samples, 2);
    assert!(!candidates.is_empty());

    let mut solver = Solver::new();
    let x = solver.declare("x");
    let y = solver.declare("y");
    let pre = simplex_constraint(solver.arena_mut(), &[x, y]).unwrap();

    let mut proved = Vec::new();
    let mut refuted = 0;
    for c in &candidates {
        match prove(&mut solver, &[x, y], pre, c).unwrap() {
            Verdict::Holds => proved.push(c.clone()),
            Verdict::Refuted { witness } => {
                refuted += 1;
                // The witness must satisfy the precondition and break the candidate.
                let sum = witness[0].checked_add(witness[1]).unwrap();
                assert_eq!(sum, Rational::from_int(10));
                assert!(witness[0] >= Rational::ZERO && witness[1] >= Rational::ZERO);
                assert!(!holds_at(c, &witness));
            }
        }
    }
    // x + y <= 10 is a true invariant and is inferred from the samples.
    let sum_bound = Candidate {
        coeffs: vec![(0, 1), (1, 1)],
        bound: Rational::from_int(10),
    };
    assert!(proved.contains(&sum_bound), "x + y <= 10 should be proved");
    // Bounds on x taken from samples in 3..=7 do not hold once x may reach 10.
    assert!(refuted > 0, "some sample-derived bounds should be refuted");
}

fn holds_at(c: &Candidate, point: &[Rational]) -> bool {
    let mut acc = Rational::ZERO;
    for &(i, k) in &c.coeffs {
        acc = acc
            .checked_add(
                point[i]
                    .checked_mul(Rational::from_int(i128::from(k)))
                    .unwrap(),
            )
            .unwrap();
    }
    acc <= c.bound
}

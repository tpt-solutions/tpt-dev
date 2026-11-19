//! Proofs of unsatisfiability for Boolean-structured formulas. Every unsat
//! answer must come with a proof that `check_proof` accepts. Tampered proofs
//! must be rejected. Sat and Unknown must give no proof.

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use common::*;
use tpt_dev_solver::{
    Budget, Farkas, Linear, Proof, ProofStep, Rational, Solver, Status, Unknown, check_proof,
};

/// Builds a random Boolean combination over two real variables in a solver.
fn setup(ast: &Ast) -> Solver {
    let mut s = Solver::new();
    let x = s.declare("x");
    let y = s.declare("y");
    let f = build_arena(s.arena_mut(), [x, y], ast);
    s.assert(f);
    s
}

/// Random cases, split by answer, with a proof for every unsat one.
fn unsat_cases(seed: u64, cases: usize, depth: u32) -> Vec<(Solver, Proof)> {
    let mut rng = Rng(seed);
    let mut out = Vec::new();
    for case in 0..cases {
        let ast = random_ast(&mut rng, depth);
        let mut s = setup(&ast);
        match s.check().unwrap() {
            Status::Sat => {
                assert_eq!(s.unsat_proof(), Ok(None), "case {case}: {ast:?}");
            }
            Status::Unsat => {
                let proof = s
                    .unsat_proof()
                    .unwrap_or_else(|e| panic!("case {case}: {e}: {ast:?}"))
                    .unwrap_or_else(|| panic!("case {case}: no proof: {ast:?}"));
                assert!(
                    check_proof(s.arena(), s.assertions(), &proof),
                    "case {case}: proof rejected: {ast:?}"
                );
                out.push((s, proof));
            }
        }
    }
    out
}

#[test]
fn random_boolean_unsat_has_checked_proofs() {
    let unsat = unsat_cases(0x5EED_0001_2345_6789, 3000, 3);
    assert!(unsat.len() > 200, "only {} unsat cases", unsat.len());
}

#[test]
fn proofs_are_sound_for_other_assertions() {
    // A checked proof shows that the assertions are unsat. So it must not
    // check against the empty assertion list, which is satisfiable.
    for (s, proof) in unsat_cases(0x0BAD_F00D_0000_0007, 400, 3) {
        assert!(!check_proof(s.arena(), &[], &proof));
    }
}

#[test]
fn dropping_the_empty_clause_is_rejected() {
    for (s, proof) in unsat_cases(0x1111_2222_3333_4444, 400, 3) {
        let mut steps = proof.steps().to_vec();
        assert_eq!(steps.pop(), Some(ProofStep::Rup(Vec::new())));
        assert!(!check_proof(s.arena(), s.assertions(), &Proof::new(steps)));
    }
}

#[test]
fn wrong_farkas_coefficient_is_rejected() {
    // Doubling one coefficient breaks the zero-sum rule for every variable of
    // that atom, so each such tampering must be rejected.
    let mut tried = 0;
    for (s, proof) in unsat_cases(0x7777_8888_9999_0000, 1500, 3) {
        for (i, step) in proof.steps().iter().enumerate() {
            let ProofStep::Theory { lits, cert } = step else {
                continue;
            };
            let mut coeffs = cert.coefficients().to_vec();
            coeffs[0].1 = coeffs[0].1.checked_add(coeffs[0].1).unwrap();
            let mut steps = proof.steps().to_vec();
            steps[i] = ProofStep::Theory {
                lits: lits.clone(),
                cert: Farkas::new(coeffs),
            };
            assert!(
                !check_proof(s.arena(), s.assertions(), &Proof::new(steps)),
                "doubled coefficient accepted at step {i}"
            );
            tried += 1;
        }
    }
    assert!(tried > 100, "only {tried} theory steps tried");
}

#[test]
fn flipped_theory_literal_is_rejected() {
    // Flipping the sign of one literal changes the zero-sum rule for its
    // atom's variables, so each such tampering must be rejected.
    let mut tried = 0;
    for (s, proof) in unsat_cases(0x3141_5926_5358_9793, 1500, 3) {
        for (i, step) in proof.steps().iter().enumerate() {
            let ProofStep::Theory { lits, cert } = step else {
                continue;
            };
            let mut lits = lits.clone();
            lits[0].1 = !lits[0].1;
            let mut steps = proof.steps().to_vec();
            steps[i] = ProofStep::Theory {
                lits,
                cert: cert.clone(),
            };
            assert!(
                !check_proof(s.arena(), s.assertions(), &Proof::new(steps)),
                "flipped literal accepted at step {i}"
            );
            tried += 1;
        }
    }
    assert!(tried > 100, "only {tried} theory steps tried");
}

#[test]
fn flipped_clause_literal_is_usually_rejected() {
    // Flipping a literal of a propositional step is not always caught,
    // because the changed clause may still follow. The checker must never
    // accept an unsound proof, so this counts rejections instead of requiring
    // all of them.
    let (mut tried, mut rejected) = (0usize, 0usize);
    for (s, proof) in unsat_cases(0x2718_2818_2845_9045, 1500, 3) {
        for (i, step) in proof.steps().iter().enumerate() {
            let ProofStep::Rup(clause) = step else {
                continue;
            };
            if clause.is_empty() {
                continue;
            }
            let mut clause = clause.clone();
            clause[0] = -clause[0];
            let mut steps = proof.steps().to_vec();
            steps[i] = ProofStep::Rup(clause);
            tried += 1;
            if !check_proof(s.arena(), s.assertions(), &Proof::new(steps)) {
                rejected += 1;
            }
        }
    }
    assert!(tried > 100, "only {tried} clause steps tried");
    assert!(rejected > 0, "no flipped clause was rejected");
    eprintln!("flipped clause literal: {rejected} of {tried} rejected");
}

#[test]
fn hand_made_proof_steps_are_checked() {
    // (x <= 0 or x >= 10) and x > 5 and x < 9.
    let mut s = Solver::new();
    let x = s.declare("x");
    let k = |n: i128| Linear::constant(Rational::from_int(n));
    let a = s.arena_mut().le(&Linear::var(x), &k(0)).unwrap();
    let b = s.arena_mut().ge(&Linear::var(x), &k(10)).unwrap();
    let or = s.arena_mut().or(vec![a, b]);
    let c = s.arena_mut().gt(&Linear::var(x), &k(5)).unwrap();
    let d = s.arena_mut().lt(&Linear::var(x), &k(9)).unwrap();
    for f in [or, c, d] {
        s.assert(f);
    }
    let proof = s.unsat_proof().unwrap().unwrap();
    assert!(check_proof(s.arena(), s.assertions(), &proof));
    // An unrelated clause is not a consequence, so it is rejected.
    let bogus = Proof::new(vec![ProofStep::Rup(vec![3]), ProofStep::Rup(Vec::new())]);
    assert!(!check_proof(s.arena(), s.assertions(), &bogus));
    // A zero literal is not a valid DIMACS literal.
    let zero = Proof::new(vec![ProofStep::Rup(vec![0]), ProofStep::Rup(Vec::new())]);
    assert!(!check_proof(s.arena(), s.assertions(), &zero));
    // An empty proof has no empty clause at its end.
    assert!(!check_proof(
        s.arena(),
        s.assertions(),
        &Proof::new(Vec::new())
    ));
}

#[test]
fn sat_gives_no_proof() {
    let mut rng = Rng(0xABCD_EF01_2345_6789);
    let mut sat = 0;
    for _ in 0..1000 {
        let ast = random_ast(&mut rng, 3);
        let mut s = setup(&ast);
        if s.check().unwrap() == Status::Sat {
            sat += 1;
            assert_eq!(s.unsat_proof(), Ok(None));
        }
    }
    assert!(sat > 100, "only {sat} sat cases");
}

#[test]
fn unknown_gives_no_proof() {
    // An unsat disjunction needs learned clauses. A clause limit of zero stops the search.
    let mut s = Solver::new();
    let x = s.declare("x");
    let k = |n: i128| Linear::constant(Rational::from_int(n));
    let a = s.arena_mut().le(&Linear::var(x), &k(0)).unwrap();
    let b = s.arena_mut().ge(&Linear::var(x), &k(10)).unwrap();
    let or = s.arena_mut().or(vec![a, b]);
    let c = s.arena_mut().gt(&Linear::var(x), &k(5)).unwrap();
    let d = s.arena_mut().lt(&Linear::var(x), &k(9)).unwrap();
    for f in [or, c, d] {
        s.assert(f);
    }
    s.set_budget(Budget {
        max_clauses: 0,
        ..Budget::UNLIMITED
    });
    assert_eq!(s.unsat_proof(), Err(Unknown));
    s.set_budget(Budget {
        max_formula_nodes: 0,
        ..Budget::UNLIMITED
    });
    assert_eq!(s.unsat_proof(), Err(Unknown));
    s.set_budget(Budget::UNLIMITED);
    assert!(s.unsat_proof().unwrap().is_some());
}

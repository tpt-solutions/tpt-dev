//! Inductive-invariant search over small transition systems.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_solver::Rational;
use tpt_dev_test::{
    Affine, Atom, Outcome, SearchConfig, Seed, SpecError, Transition, TransitionSystem,
    find_invariant,
};

fn config() -> SearchConfig {
    SearchConfig::default()
}

/// Counter: x starts at 0 and steps up by 1 while x <= 9.
fn bounded_counter(property_bound: i64) -> TransitionSystem {
    TransitionSystem::new(
        1,
        vec![Atom::eq(&[(0, 1)], 0)],
        vec![Transition::new(
            vec![Atom::le(&[(0, 1)], 9)],
            vec![Affine::new(&[(0, 1)], 1)],
        )],
        vec![Atom::le(&[(0, 1)], property_bound)],
    )
    .unwrap()
}

#[test]
fn bounded_counter_is_proven() {
    let outcome = find_invariant(&bounded_counter(10), &config());
    match outcome {
        Outcome::Proven { invariant } => assert!(!invariant.is_empty()),
        other => panic!("expected Proven, got {other:?}"),
    }
}

#[test]
fn sum_invariant_is_proven() {
    // x starts at 0, y at 10. Each step moves one unit from y to x while y >= 1.
    // x + y == 10 is preserved, and the property is that equality.
    let system = TransitionSystem::new(
        2,
        vec![Atom::eq(&[(0, 1)], 0), Atom::eq(&[(1, 1)], 10)],
        vec![Transition::new(
            vec![Atom::ge(&[(1, 1)], 1)],
            vec![Affine::new(&[(0, 1)], 1), Affine::new(&[(1, 1)], -1)],
        )],
        vec![Atom::eq(&[(0, 1), (1, 1)], 10)],
    )
    .unwrap();
    assert!(matches!(
        find_invariant(&system, &config()),
        Outcome::Proven { .. }
    ));
}

#[test]
fn relational_invariant_is_proven() {
    // x and y both start at 0 and step together while x <= 4. The property
    // x == y is relational: neither variable has a bound that implies it.
    let system = TransitionSystem::new(
        2,
        vec![Atom::eq(&[(0, 1)], 0), Atom::eq(&[(1, 1)], 0)],
        vec![Transition::new(
            vec![Atom::le(&[(0, 1)], 4)],
            vec![Affine::new(&[(0, 1)], 1), Affine::new(&[(1, 1)], 1)],
        )],
        vec![Atom::eq(&[(0, 1), (1, -1)], 0)],
    )
    .unwrap();
    assert!(matches!(
        find_invariant(&system, &config()),
        Outcome::Proven { .. }
    ));
}

#[test]
fn false_property_is_violated_not_proven() {
    // The counter reaches 19, so x <= 10 is false. The walk must find a violation.
    let system = TransitionSystem::new(
        1,
        vec![Atom::eq(&[(0, 1)], 0)],
        vec![Transition::new(
            vec![Atom::le(&[(0, 1)], 19)],
            vec![Affine::new(&[(0, 1)], 1)],
        )],
        vec![Atom::le(&[(0, 1)], 10)],
    )
    .unwrap();
    match find_invariant(&system, &config()) {
        Outcome::Violated { trace } => {
            let last = trace.last().unwrap();
            assert_eq!(last[0], Rational::from_int(11));
            assert_eq!(trace[0][0], Rational::ZERO);
        }
        other => panic!("expected Violated, got {other:?}"),
    }
}

#[test]
fn violation_behind_a_jump_is_found() {
    // Counting up to 10 is safe. From 10 the second transition jumps to 50,
    // which breaks x <= 10. The walk is deterministic here, so it must find it.
    let system = TransitionSystem::new(
        1,
        vec![Atom::eq(&[(0, 1)], 0)],
        vec![
            Transition::new(
                vec![Atom::le(&[(0, 1)], 9)],
                vec![Affine::new(&[(0, 1)], 1)],
            ),
            Transition::new(vec![Atom::ge(&[(0, 1)], 10)], vec![Affine::new(&[], 50)]),
        ],
        vec![Atom::le(&[(0, 1)], 10)],
    )
    .unwrap();
    assert!(matches!(
        find_invariant(&system, &config()),
        Outcome::Violated { .. }
    ));
}

#[test]
fn unsatisfiable_init_is_vacuous() {
    let system = TransitionSystem::new(
        1,
        vec![Atom::ge(&[(0, 1)], 1), Atom::le(&[(0, 1)], 0)],
        vec![],
        vec![Atom::le(&[(0, 1)], 0)],
    )
    .unwrap();
    assert_eq!(find_invariant(&system, &config()), Outcome::Vacuous);
}

#[test]
fn search_is_deterministic_for_a_seed() {
    let system = bounded_counter(10);
    let a = find_invariant(
        &system,
        &SearchConfig {
            seed: Seed(7),
            ..config()
        },
    );
    let b = find_invariant(
        &system,
        &SearchConfig {
            seed: Seed(7),
            ..config()
        },
    );
    assert_eq!(a, b);
}

#[test]
fn malformed_systems_are_rejected() {
    let bad_index = TransitionSystem::new(1, vec![Atom::le(&[(1, 1)], 0)], vec![], vec![]);
    assert_eq!(bad_index, Err(SpecError::VariableOutOfRange));

    let bad_updates = TransitionSystem::new(
        2,
        vec![],
        vec![Transition::new(vec![], vec![Affine::var(0)])],
        vec![],
    );
    assert_eq!(bad_updates, Err(SpecError::UpdateCount));
}

#[test]
fn property_needing_a_mined_helper_is_proven() {
    // x counts up to 10. y copies x one step behind. The property y <= 10 is not
    // inductive by itself: it needs x <= 10, which is a bound mined from samples.
    let system = TransitionSystem::new(
        2,
        vec![Atom::eq(&[(0, 1)], 0), Atom::eq(&[(1, 1)], 0)],
        vec![Transition::new(
            vec![Atom::le(&[(0, 1)], 9)],
            vec![Affine::new(&[(0, 1)], 1), Affine::var(0)],
        )],
        vec![Atom::le(&[(1, 1)], 10)],
    )
    .unwrap();
    match find_invariant(&system, &config()) {
        Outcome::Proven { invariant } => {
            assert!(
                invariant
                    .iter()
                    .any(|c| c.coeffs == vec![(0, 1)] && c.bound == Rational::from_int(10))
            );
        }
        other => panic!("expected Proven, got {other:?}"),
    }
}

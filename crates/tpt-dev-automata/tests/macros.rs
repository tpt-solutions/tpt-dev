//! The `machine!` front end: each comparison, and agreement with the builder API.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_automata::{DeadlockKind, Linear, Machine, Rational, machine};

fn zero() -> Linear {
    Linear::constant(Rational::ZERO)
}

#[test]
fn complementary_pairs_leave_no_gap() {
    machine! {
        lt_ge {
            vars: [x];
            states: [s, done];
            finals: [done];
            transitions: [
                s -> done : "lt" when x < 5;
                s -> done : "ge" when x >= 5;
            ];
        }
    }
    assert!(lt_ge.analyze(s).unwrap().is_deadlock_free());

    machine! {
        le_gt {
            vars: [x];
            states: [s, done];
            finals: [done];
            transitions: [
                s -> done : "le" when x <= 5;
                s -> done : "gt" when x > 5;
            ];
        }
    }
    assert!(le_gt.analyze(s).unwrap().is_deadlock_free());

    machine! {
        eq_ne {
            vars: [x];
            states: [s, done];
            finals: [done];
            transitions: [
                s -> done : "eq" when x == -2;
                s -> done : "ne" when x != -2;
            ];
        }
    }
    assert!(eq_ne.analyze(s).unwrap().is_deadlock_free());
}

#[test]
fn a_gap_is_reported_with_a_witness() {
    machine! {
        gap {
            vars: [x];
            states: [s, done];
            finals: [done];
            transitions: [
                s -> done : "gt" when x > 5;
                s -> done : "lt" when x < 5;
            ];
        }
    }
    let report = gap.analyze(s).unwrap();
    assert_eq!(report.deadlocks.len(), 1);
    match &report.deadlocks[0].kind {
        DeadlockKind::GuardsIncomplete { witness } => {
            // Over the reals the only value outside both guards is 5.
            assert_eq!(witness.len(), 1);
            assert_eq!(witness[0].0, "x");
            assert_eq!(witness[0].1, Rational::from_int(5));
        }
        other => panic!("unexpected deadlock kind: {other:?}"),
    }
}

#[test]
fn ne_alone_is_stuck_at_the_excluded_value() {
    machine! {
        only_ne {
            vars: [x];
            states: [s, done];
            finals: [done];
            transitions: [
                s -> done : "ne" when x != 3;
            ];
        }
    }
    let report = only_ne.analyze(s).unwrap();
    match &report.deadlocks[0].kind {
        DeadlockKind::GuardsIncomplete { witness } => {
            assert_eq!(witness[0].1, Rational::from_int(3));
        }
        other => panic!("unexpected deadlock kind: {other:?}"),
    }
}

#[test]
fn unguarded_transition_and_no_transition_cases() {
    machine! {
        open {
            vars: [x];
            states: [s, done];
            finals: [done];
            transitions: [ s -> done : "go"; ];
        }
    }
    assert!(open.analyze(s).unwrap().is_deadlock_free());

    machine! {
        dead {
            vars: [];
            states: [s, done];
            finals: [done];
            transitions: [];
        }
    }
    let report = dead.analyze(s).unwrap();
    assert_eq!(report.deadlocks[0].kind, DeadlockKind::NoTransitions);
}

#[test]
fn macro_matches_the_builder() {
    machine! {
        traffic {
            vars: [x];
            states: [idle, busy];
            finals: [idle];
            transitions: [
                idle -> busy : "start" when x > 0;
                busy -> idle : "stop" when x <= 0;
                busy -> busy : "tick" when x > 0;
            ];
        }
    }
    let from_macro = traffic.analyze(idle).unwrap();

    let mut b = Machine::new();
    let bx = b.declare("x");
    let bidle = b.state("idle");
    let bbusy = b.state("busy");
    b.mark_final(bidle);
    let pos = b.arena_mut().gt(&Linear::var(bx), &zero()).unwrap();
    let nonpos = b.arena_mut().le(&Linear::var(bx), &zero()).unwrap();
    b.transition(bidle, bbusy, "start", Some(pos)).unwrap();
    b.transition(bbusy, bidle, "stop", Some(nonpos)).unwrap();
    b.transition(bbusy, bbusy, "tick", Some(pos)).unwrap();
    let from_builder = b.analyze(bidle).unwrap();

    assert_eq!(from_macro, from_builder);
    assert!(from_macro.is_deadlock_free());
}

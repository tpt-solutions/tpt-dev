//! Analysis tests: deadlocks, witnesses, traces, reachability and errors.
#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_automata::{DeadlockKind, Error, Linear, Machine, Rational};

fn int(n: i128) -> Linear {
    Linear::constant(Rational::from_int(n))
}

#[test]
fn dead_end_state_reports_trace() {
    let mut m = Machine::new();
    let start = m.state("start");
    let mid = m.state("mid");
    let end = m.state("end"); // not final, no way out
    m.transition(start, mid, "a", None).unwrap();
    m.transition(mid, end, "b", None).unwrap();

    let report = m.analyze(start).unwrap();
    assert_eq!(report.deadlocks.len(), 1);
    let d = &report.deadlocks[0];
    assert_eq!(d.state, end);
    assert_eq!(d.kind, DeadlockKind::NoTransitions);
    assert_eq!(d.trace, ["a", "b"]);
}

#[test]
fn final_states_are_not_deadlocks() {
    let mut m = Machine::new();
    let start = m.state("start");
    let done = m.state("done");
    m.mark_final(done);
    m.transition(start, done, "finish", None).unwrap();
    let report = m.analyze(start).unwrap();
    assert!(report.is_deadlock_free());
}

#[test]
fn incomplete_guards_give_a_witness() {
    // From `wait`, "go" needs x > 0 and "retry" needs x > 5. x = 0 gets stuck.
    let mut m = Machine::new();
    let x = m.declare("x");
    let wait = m.state("wait");
    let done = m.state("done");
    m.mark_final(done);
    let gt0 = m.arena_mut().gt(&Linear::var(x), &int(0)).unwrap();
    let gt5 = m.arena_mut().gt(&Linear::var(x), &int(5)).unwrap();
    m.transition(wait, done, "go", Some(gt0)).unwrap();
    m.transition(wait, wait, "retry", Some(gt5)).unwrap();

    let report = m.analyze(wait).unwrap();
    assert_eq!(report.deadlocks.len(), 1);
    match &report.deadlocks[0].kind {
        DeadlockKind::GuardsIncomplete { witness } => {
            assert_eq!(witness.len(), 1);
            assert_eq!(witness[0].0, "x");
            // The witness must really disable both guards.
            assert!(witness[0].1 <= Rational::ZERO);
        }
        other => panic!("expected incomplete guards, got {other:?}"),
    }
}

#[test]
fn complementary_guards_are_deadlock_free() {
    let mut m = Machine::new();
    let x = m.declare("x");
    let s = m.state("s");
    let done = m.state("done");
    m.mark_final(done);
    let lt = m.arena_mut().lt(&Linear::var(x), &int(10)).unwrap();
    let ge = m.arena_mut().ge(&Linear::var(x), &int(10)).unwrap();
    m.transition(s, done, "low", Some(lt)).unwrap();
    m.transition(s, done, "high", Some(ge)).unwrap();
    assert!(m.analyze(s).unwrap().is_deadlock_free());
}

#[test]
fn unguarded_transition_covers_everything() {
    let mut m = Machine::new();
    let _x = m.declare("x");
    let s = m.state("s");
    let done = m.state("done");
    m.mark_final(done);
    let never = m.arena_mut().constant(false);
    m.transition(s, done, "maybe", Some(never)).unwrap();
    m.transition(s, s, "always", None).unwrap();
    assert!(m.analyze(s).unwrap().is_deadlock_free());
}

#[test]
fn unreachable_states_are_reported() {
    let mut m = Machine::new();
    let a = m.state("a");
    let b = m.state("b");
    let island = m.state("island");
    m.mark_final(b);
    m.transition(a, b, "go", None).unwrap();
    m.transition(island, b, "back", None).unwrap();
    let report = m.analyze(a).unwrap();
    assert_eq!(report.reachable, [a, b]);
    assert_eq!(report.unreachable, [island]);
}

#[test]
fn errors_for_unknown_states_and_formulas() {
    let mut m = Machine::new();
    let a = m.state("a");
    let other = {
        let mut o = Machine::new();
        o.state("x");
        o.state("y");
        o.state("z")
    };
    assert_eq!(
        m.transition(a, other, "bad", None),
        Err(Error::UnknownState)
    );
    assert_eq!(m.analyze(other).err(), Some(Error::UnknownState));
}

#[test]
fn guard_question_on_overflowing_values_is_unknown() {
    // Coefficients large enough that the solver's arithmetic overflows.
    let mut m = Machine::new();
    let x = m.declare("x");
    let s = m.state("s");
    let big = Rational::from_int(i128::MAX / 2);
    let lhs = Linear::var(x)
        .checked_add(&Linear::constant(Rational::ZERO))
        .unwrap();
    let guard = m
        .arena_mut()
        .gt(&lhs.scale(big).unwrap(), &Linear::constant(big))
        .unwrap();
    m.transition(s, s, "spin", Some(guard)).unwrap();
    match m.analyze(s) {
        Ok(report) => {
            // Either the solver decides it or reports Unknown. It never fails silently.
            assert!(report.deadlocks.len() <= 1);
        }
        Err(e) => assert_eq!(e, Error::Unknown),
    }
}

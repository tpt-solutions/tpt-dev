//! Solver-seeded fuzzing reaches a bug behind a linear equality that plain
//! coverage-guided fuzzing does not reach within the same budget.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_fuzz::{Config, Coverage, Fuzzer, Outcome, minimise, solver_seeds};
use tpt_dev_solver::{Linear, Rational};

/// Four inputs. The target has a bug when they sum to 1234 with a large first input.
///
/// Coverage edge 1 marks the large first input, which is reached by about a
/// quarter of random inputs. Edge 2 is the bug itself, which random search
/// almost never hits because it needs the exact sum.
fn target(v: &[i64], cov: &mut Coverage) -> Outcome {
    cov.hit(0);
    if v[0] > 1000 {
        cov.hit(1);
        if v[0] + v[1] + v[2] + v[3] == 1234 {
            cov.hit(2);
            return Outcome::Crash(String::from("sum hits 1234 with a large first input"));
        }
    }
    Outcome::Pass
}

fn budget() -> Config {
    Config {
        iterations: 20_000,
        ..Config::default()
    }
}

#[test]
fn plain_fuzzing_misses_the_bug_within_budget() {
    let mut fuzzer = Fuzzer::new(target, 4, budget());
    assert!(
        fuzzer.run(&[]).is_none(),
        "plain fuzzing should not find the exact sum"
    );
}

#[test]
fn solver_seeded_fuzzing_finds_and_minimises_the_bug() {
    // a + b + c + d = 1234 and a > 1000, solved over the reals and kept where integral.
    let seeds = solver_seeds(4, 16, |arena, v| {
        let mut sum = Linear::var(v[0]);
        for &x in &v[1..] {
            sum = sum.checked_add(&Linear::var(x))?;
        }
        let eq = arena.eq(&sum, &Linear::constant(Rational::from_int(1234)))?;
        let big = arena.gt(
            &Linear::var(v[0]),
            &Linear::constant(Rational::from_int(1000)),
        )?;
        Some(arena.and(vec![eq, big]))
    })
    .unwrap();
    assert!(!seeds.is_empty());
    assert!(
        seeds
            .iter()
            .all(|s| s[0] > 1000 && s.iter().sum::<i64>() == 1234)
    );

    let mut fuzzer = Fuzzer::new(target, 4, budget());
    let found = fuzzer.run(&seeds).expect("seeds satisfy the bug condition");
    assert_eq!(found.input.iter().sum::<i64>(), 1234);
    assert!(found.input[0] > 1000);
    assert_eq!(found.iterations, 0, "a seed is already a crash");
}

#[test]
fn minimisation_keeps_the_crash_and_shrinks_inputs() {
    // Start from a crashing input with large, uneven values.
    let mut t = target;
    let start = vec![5000, 3000, -2000, -4766];
    let (min, message) = minimise(&mut t, &start).unwrap();
    assert_eq!(min.iter().sum::<i64>(), 1234);
    assert!(min[0] > 1000);
    assert!(message.contains("1234"));
    // Every coordinate is no further from zero than it started.
    for (m, s) in min.iter().zip(&start) {
        assert!(m.abs() <= s.abs());
    }
}

#[test]
fn non_crashing_input_does_not_minimise() {
    let mut t = target;
    assert!(minimise(&mut t, &[1, 2, 3, 4]).is_none());
}

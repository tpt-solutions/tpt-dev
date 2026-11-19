//! Timings for the solver on random QF_LRA instances.
//!
//! Run with `cargo run -p tpt-dev-solver --release --example bench`.
//! Instances are generated from fixed seeds, so the results are repeatable.
//! Each row gives the time for one `check` on a fresh solver, averaged and
//! worst-case over the seeds, and the number of answers of each kind.

#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use tpt_dev_solver::{Linear, Rational, Solver, Status, Var};

/// A xorshift generator with a fixed seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

/// A random instance: `clauses` disjunctions of `width` random linear atoms,
/// each over two of `vars` variables.
fn disjunctive(seed: u64, vars: usize, clauses: usize, width: usize) -> Solver {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut s = Solver::new();
    let xs: Vec<Var> = (0..vars).map(|i| s.declare(&format!("x{i}"))).collect();
    for _ in 0..clauses {
        let mut lits = Vec::with_capacity(width);
        for _ in 0..width {
            let i = rng.range(0, vars as i64 - 1) as usize;
            let j = rng.range(0, vars as i64 - 1) as usize;
            let lhs = Linear::var(xs[i])
                .scale(Rational::from_int(rng.range(-3, 3) as i128))
                .unwrap()
                .checked_add(
                    &Linear::var(xs[j])
                        .scale(Rational::from_int(rng.range(-3, 3) as i128))
                        .unwrap(),
                )
                .unwrap();
            let rhs = Linear::constant(Rational::from_int(rng.range(-10, 10) as i128));
            let atom = if rng.range(0, 1) == 0 {
                s.arena_mut().le(&lhs, &rhs)
            } else {
                s.arena_mut().gt(&lhs, &rhs)
            };
            lits.push(atom.unwrap());
        }
        let clause = s.arena_mut().or(lits);
        s.assert(clause);
    }
    s
}

/// A random conjunction of `atoms` linear atoms over `vars` variables.
fn conjunctive(seed: u64, vars: usize, atoms: usize) -> Solver {
    disjunctive(seed, vars, atoms, 1)
}

struct Row {
    label: String,
    mean: Duration,
    worst: Duration,
    sat: usize,
    unsat: usize,
    unknown: usize,
}

fn measure(label: &str, seeds: u64, build: impl Fn(u64) -> Solver) -> Row {
    let mut total = Duration::ZERO;
    let mut worst = Duration::ZERO;
    let (mut sat, mut unsat, mut unknown) = (0, 0, 0);
    for seed in 1..=seeds {
        let mut s = build(seed);
        let start = Instant::now();
        let answer = s.check();
        let took = start.elapsed();
        total += took;
        worst = worst.max(took);
        match answer {
            Ok(Status::Sat) => sat += 1,
            Ok(Status::Unsat) => unsat += 1,
            Err(_) => unknown += 1,
        }
    }
    Row {
        label: label.to_string(),
        mean: total / seeds as u32,
        worst,
        sat,
        unsat,
        unknown,
    }
}

fn main() {
    const SEEDS: u64 = 10;
    let mut rows = Vec::new();
    for &(vars, clauses) in &[(5, 10), (10, 25), (15, 40), (20, 60)] {
        rows.push(measure(
            &format!("disjunctive v={vars} clauses={clauses} width=2"),
            SEEDS,
            |seed| disjunctive(seed, vars, clauses, 2),
        ));
    }
    for &(vars, atoms) in &[(10, 30), (30, 90), (60, 180)] {
        rows.push(measure(
            &format!("conjunctive  v={vars} atoms={atoms}"),
            SEEDS,
            |seed| conjunctive(seed, vars, atoms),
        ));
    }

    println!(
        "{:<42} {:>10} {:>10} {:>5} {:>6} {:>8}",
        "instance (per check)", "mean ms", "worst ms", "sat", "unsat", "unknown"
    );
    for r in &rows {
        println!(
            "{:<42} {:>10.3} {:>10.3} {:>5} {:>6} {:>8}",
            r.label,
            r.mean.as_secs_f64() * 1e3,
            r.worst.as_secs_f64() * 1e3,
            r.sat,
            r.unsat,
            r.unknown
        );
    }
}

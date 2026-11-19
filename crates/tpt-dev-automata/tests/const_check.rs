//! The const check must agree with the solver-backed run-time analysis.
#![allow(clippy::unwrap_used, clippy::panic, clippy::cast_possible_truncation)]

use tpt_dev_automata::const_check::{
    Atom, Cmp, Edge, Spec, Verdict, assert_deadlock_free, check_deadlock_free,
};
use tpt_dev_automata::{DeadlockKind, Linear, Machine, Rational, StateId};

// Compile-time acceptance: this file fails to build if the check rejects it.
const SAFE_GUARDS: &[Atom] = &[Atom::new(0, Cmp::Gt, 0)];
const SAFE_COMPLEMENT: &[Atom] = &[Atom::new(0, Cmp::Le, 0)];
const SAFE_EDGES: &[Edge<'_>] = &[
    Edge {
        from: 0,
        to: 1,
        guard: SAFE_GUARDS,
    },
    Edge {
        from: 0,
        to: 0,
        guard: SAFE_COMPLEMENT,
    },
];
const SAFE: Spec<'_> = Spec {
    states: 2,
    vars: 1,
    initial: 0,
    finals: &[1],
    edges: SAFE_EDGES,
};
const _: () = assert_deadlock_free(&SAFE);

/// An owned machine description, borrowed as a [`Spec`].
struct Owned {
    states: usize,
    vars: usize,
    finals: Vec<usize>,
    edges: Vec<(usize, usize, Vec<Atom>)>,
}

/// Builds the same machine with the solver-backed API.
fn to_machine(o: &Owned) -> (Machine, Vec<StateId>) {
    let mut m = Machine::new();
    let vars: Vec<_> = (0..o.vars).map(|i| m.declare(&format!("x{i}"))).collect();
    let states: Vec<_> = (0..o.states).map(|i| m.state(&format!("s{i}"))).collect();
    for &f in &o.finals {
        m.mark_final(states[f]);
    }
    for (from, to, atoms) in &o.edges {
        let guard = if atoms.is_empty() {
            None
        } else {
            let mut fs = Vec::new();
            for a in atoms {
                let lhs = Linear::var(vars[a.var()]);
                let rhs = Linear::constant(Rational::from_int(a.value() as i128));
                let arena = m.arena_mut();
                let f = match a.cmp() {
                    Cmp::Lt => arena.lt(&lhs, &rhs),
                    Cmp::Le => arena.le(&lhs, &rhs),
                    Cmp::Gt => arena.gt(&lhs, &rhs),
                    Cmp::Ge => arena.ge(&lhs, &rhs),
                    Cmp::Eq => arena.eq(&lhs, &rhs),
                }
                .unwrap();
                fs.push(f);
            }
            Some(m.arena_mut().and(fs))
        };
        m.transition(states[*from], states[*to], "e", guard)
            .unwrap();
    }
    (m, states)
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n
    }
}

fn random_machine(rng: &mut Lcg) -> Owned {
    let states = 1 + rng.next(5) as usize;
    let vars = 1 + rng.next(2) as usize;
    let finals = (0..states).filter(|_| rng.next(4) == 0).collect::<Vec<_>>();
    let n_edges = rng.next(9) as usize;
    let mut edges = Vec::new();
    for _ in 0..n_edges {
        let from = rng.next(states as u64) as usize;
        let to = rng.next(states as u64) as usize;
        let n_atoms = if rng.next(4) == 0 {
            0
        } else {
            1 + rng.next(2) as usize
        };
        let atoms = (0..n_atoms)
            .map(|_| {
                let cmp = [Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge, Cmp::Eq][rng.next(5) as usize];
                let value = rng.next(7) as i64 - 3;
                Atom::new(rng.next(vars as u64) as usize, cmp, value)
            })
            .collect();
        edges.push((from, to, atoms));
    }
    Owned {
        states,
        vars,
        finals,
        edges,
    }
}

#[test]
fn const_check_agrees_with_runtime_analysis() {
    let mut rng = Lcg(0x5eed_1234);
    let mut free = 0;
    let mut stuck = 0;
    for _ in 0..3000 {
        let owned = random_machine(&mut rng);
        let edges: Vec<Edge<'_>> = owned
            .edges
            .iter()
            .map(|(from, to, guard)| Edge {
                from: *from,
                to: *to,
                guard: guard.as_slice(),
            })
            .collect();
        let spec = Spec {
            states: owned.states,
            vars: owned.vars,
            initial: 0,
            finals: &owned.finals,
            edges: &edges,
        };

        let verdict = check_deadlock_free(&spec);
        let (mut machine, states) = to_machine(&owned);
        let report = machine.analyze(states[0]).unwrap();

        match verdict {
            Verdict::DeadlockFree => {
                free += 1;
                assert!(
                    report.is_deadlock_free(),
                    "const says free, solver disagrees"
                );
            }
            Verdict::NoTransitions { state } => {
                stuck += 1;
                let d = report
                    .deadlocks
                    .iter()
                    .find(|d| d.state == states[state])
                    .unwrap_or_else(|| panic!("solver missed stuck state {state}"));
                assert_eq!(d.kind, DeadlockKind::NoTransitions);
            }
            Verdict::GuardsIncomplete { state } => {
                stuck += 1;
                let d = report
                    .deadlocks
                    .iter()
                    .find(|d| d.state == states[state])
                    .unwrap_or_else(|| panic!("solver missed stuck state {state}"));
                assert!(matches!(d.kind, DeadlockKind::GuardsIncomplete { .. }));
            }
            other => panic!("generator stays in range, got {other:?}"),
        }
    }
    // Both outcomes must occur, or the test proves nothing.
    assert!(free > 100 && stuck > 100, "free={free} stuck={stuck}");
}

fn spec_with<'a>(vars: usize, edges: &'a [Edge<'a>]) -> Spec<'a> {
    Spec {
        states: 2,
        vars,
        initial: 0,
        finals: &[1],
        edges,
    }
}

#[test]
fn one_variable_coverage_is_exact() {
    let pos = [Atom::new(0, Cmp::Gt, 0)];
    let neg = [Atom::new(0, Cmp::Lt, 0)];
    let nonpos = [Atom::new(0, Cmp::Le, 0)];
    let nonneg = [Atom::new(0, Cmp::Ge, 0)];
    let eq0 = [Atom::new(0, Cmp::Eq, 0)];

    // x > 0 or x < 0 misses x = 0.
    let gap = [
        Edge {
            from: 0,
            to: 1,
            guard: &pos,
        },
        Edge {
            from: 0,
            to: 1,
            guard: &neg,
        },
    ];
    assert_eq!(
        check_deadlock_free(&spec_with(1, &gap)),
        Verdict::GuardsIncomplete { state: 0 }
    );

    // x > 0 or x <= 0 covers everything, including the boundary.
    let full = [
        Edge {
            from: 0,
            to: 1,
            guard: &pos,
        },
        Edge {
            from: 0,
            to: 1,
            guard: &nonpos,
        },
    ];
    assert_eq!(
        check_deadlock_free(&spec_with(1, &full)),
        Verdict::DeadlockFree
    );

    // x >= 0 or x < 0 covers everything. x == 0 alone does not.
    let split = [
        Edge {
            from: 0,
            to: 1,
            guard: &nonneg,
        },
        Edge {
            from: 0,
            to: 1,
            guard: &neg,
        },
    ];
    assert_eq!(
        check_deadlock_free(&spec_with(1, &split)),
        Verdict::DeadlockFree
    );
    let point = [Edge {
        from: 0,
        to: 1,
        guard: &eq0,
    }];
    assert_eq!(
        check_deadlock_free(&spec_with(1, &point)),
        Verdict::GuardsIncomplete { state: 0 }
    );
}

#[test]
fn two_variable_boxes_are_covered_by_grid() {
    let x_ge = Atom::new(0, Cmp::Ge, 0);
    let y_ge = Atom::new(1, Cmp::Ge, 0);
    let x_lt = Atom::new(0, Cmp::Lt, 0);
    let y_lt = Atom::new(1, Cmp::Lt, 0);
    let quadrant = [x_ge, y_ge];
    let left = [x_lt];
    let below = [y_lt];
    let cover = [
        Edge {
            from: 0,
            to: 1,
            guard: &quadrant,
        },
        Edge {
            from: 0,
            to: 1,
            guard: &left,
        },
        Edge {
            from: 0,
            to: 1,
            guard: &below,
        },
    ];
    assert_eq!(
        check_deadlock_free(&spec_with(2, &cover)),
        Verdict::DeadlockFree
    );

    // Without the "below" box, x >= 0 and y < 0 is uncovered.
    let missing = [
        Edge {
            from: 0,
            to: 1,
            guard: &quadrant,
        },
        Edge {
            from: 0,
            to: 1,
            guard: &left,
        },
    ];
    assert_eq!(
        check_deadlock_free(&spec_with(2, &missing)),
        Verdict::GuardsIncomplete { state: 0 }
    );
}

#[test]
fn unreachable_stuck_states_are_ignored() {
    // State 2 has no way out, but nothing reaches it.
    let always: [Atom; 0] = [];
    let edges = [
        Edge {
            from: 0,
            to: 1,
            guard: &always,
        },
        Edge {
            from: 2,
            to: 0,
            guard: &always,
        },
    ];
    let spec = Spec {
        states: 3,
        vars: 0,
        initial: 0,
        finals: &[1],
        edges: &edges,
    };
    assert_eq!(check_deadlock_free(&spec), Verdict::DeadlockFree);
}

#[test]
fn malformed_specs_are_reported() {
    let always: [Atom; 0] = [];
    let bad_state = [Edge {
        from: 0,
        to: 5,
        guard: &always,
    }];
    assert_eq!(
        check_deadlock_free(&spec_with(0, &bad_state)),
        Verdict::OutOfRange
    );
    let bad_var = [Edge {
        from: 0,
        to: 1,
        guard: &[Atom::new(3, Cmp::Gt, 0)],
    }];
    assert_eq!(
        check_deadlock_free(&spec_with(1, &bad_var)),
        Verdict::OutOfRange
    );
    let too_many_vars = Spec {
        states: 1,
        vars: 99,
        initial: 0,
        finals: &[],
        edges: &[],
    };
    assert_eq!(check_deadlock_free(&too_many_vars), Verdict::TooLarge);
}

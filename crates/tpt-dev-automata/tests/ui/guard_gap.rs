use tpt_dev_automata::const_check::{assert_deadlock_free, Atom, Cmp, Edge, Spec};

// x > 0 or x < 0 leaves x = 0 uncovered, so state 0 can get stuck.
const POS: &[Atom] = &[Atom::new(0, Cmp::Gt, 0)];
const NEG: &[Atom] = &[Atom::new(0, Cmp::Lt, 0)];
const EDGES: &[Edge<'_>] = &[
    Edge { from: 0, to: 1, guard: POS },
    Edge { from: 0, to: 1, guard: NEG },
];
const SPEC: Spec<'_> = Spec {
    states: 2,
    vars: 1,
    initial: 0,
    finals: &[1],
    edges: EDGES,
};
const _: () = assert_deadlock_free(&SPEC);

fn main() {}

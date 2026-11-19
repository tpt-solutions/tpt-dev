use tpt_dev_automata::const_check::{assert_deadlock_free, Edge, Spec};

// State 1 is reachable, not final, and has no way out.
const EDGES: &[Edge<'_>] = &[Edge {
    from: 0,
    to: 1,
    guard: &[],
}];
const SPEC: Spec<'_> = Spec {
    states: 2,
    vars: 0,
    initial: 0,
    finals: &[],
    edges: EDGES,
};
const _: () = assert_deadlock_free(&SPEC);

fn main() {}

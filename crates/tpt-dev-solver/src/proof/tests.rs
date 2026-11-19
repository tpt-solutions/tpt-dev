//! Tests for the Tseitin encoder behind [`check_proof`](super::check_proof).
//!
//! The encoder is checked against the formula it encodes, by enumerating
//! every assignment to the Boolean variables of its CNF. The checker is then
//! checked against a base from a deliberately broken encoder, which is written
//! here and never used by the library.

#![allow(clippy::unwrap_used, clippy::panic)]

#[path = "../../tests/common/mod.rs"]
mod common;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use common::*;

use super::{Base, Proof, ProofStep, check_proof, check_steps};
use crate::rational::Rational;
use crate::sat::Lit;
use crate::solver::farkas_conflict;
use crate::term::{Arena, Formula, Linear, NodeView, Source, Var};

/// Largest CNF checked by enumerating every assignment: `2^16` of them.
const MAX_VARS: usize = 16;

/// Truth value of an atom with no variables, from its bound alone.
fn constant_atom(bound: Rational, strict: bool) -> bool {
    // `0 <= bound`, or `0 < bound` when strict.
    !bound.is_negative() && (!strict || !bound.is_zero())
}

/// Evaluates `f` with each linear atom taking the value in `atom_val`.
/// Returns `None` if a linear atom has no value.
fn eval_bool(arena: &Arena, f: Formula, atom_val: &BTreeMap<u32, bool>) -> Option<bool> {
    Some(match arena.view(f) {
        NodeView::Const(b) => b,
        NodeView::Atom {
            terms,
            bound,
            strict,
        } => {
            if terms.is_empty() {
                constant_atom(bound, strict)
            } else {
                *atom_val.get(&f.0)?
            }
        }
        NodeView::Not(g) => !eval_bool(arena, g, atom_val)?,
        NodeView::And(gs) => gs
            .iter()
            .map(|&g| eval_bool(arena, g, atom_val))
            .collect::<Option<Vec<bool>>>()?
            .iter()
            .all(|&b| b),
        NodeView::Or(gs) => gs
            .iter()
            .map(|&g| eval_bool(arena, g, atom_val))
            .collect::<Option<Vec<bool>>>()?
            .iter()
            .any(|&b| b),
    })
}

/// Checks that `base` encodes `root` exactly. For every assignment to the
/// linear atoms, the CNF has exactly one satisfying extension to the other
/// variables if `root` is true under that assignment, and none if it is false.
/// Exactly one means the auxiliary values are fixed by the atoms, so the
/// encoder's clauses define them and nothing else.
///
/// Every assignment to all variables is enumerated, so `base.n_vars` must be
/// at most [`MAX_VARS`].
fn encodes_exactly(arena: &Arena, root: Formula, base: &Base) -> Result<(), String> {
    let n = base.n_vars;
    assert!(n <= MAX_VARS, "CNF has {n} variables");
    // Atoms in a fixed order, with their variables.
    let atoms: Vec<(usize, Formula)> = base
        .atom_var
        .iter()
        .map(|(&f, &v)| (v as usize, Formula(f)))
        .collect();
    // Each clause as (positive variables, negative variables), as bit masks.
    let clauses: Vec<(u32, u32)> = base
        .clauses
        .iter()
        .map(|c| {
            let mut pos = 0u32;
            let mut neg = 0u32;
            for l in c {
                if l.is_negative() {
                    neg |= 1 << l.var();
                } else {
                    pos |= 1 << l.var();
                }
            }
            (pos, neg)
        })
        .collect();

    // count[key] is the number of satisfying assignments whose atoms are `key`.
    let mut count = vec![0u32; 1 << atoms.len()];
    for sigma in 0u32..(1 << n) {
        let sat = clauses
            .iter()
            .all(|&(pos, neg)| (sigma & pos) != 0 || (!sigma & neg) != 0);
        if !sat {
            continue;
        }
        let mut key = 0usize;
        for (i, &(v, _)) in atoms.iter().enumerate() {
            if (sigma >> v) & 1 == 1 {
                key |= 1 << i;
            }
        }
        count[key] += 1;
    }

    for (key, &got) in count.iter().enumerate() {
        let vals: BTreeMap<u32, bool> = atoms
            .iter()
            .enumerate()
            .map(|(i, &(_, f))| (f.0, (key >> i) & 1 == 1))
            .collect();
        let expect = eval_bool(arena, root, &vals)
            .ok_or_else(|| String::from("a linear atom has no CNF variable"))?;
        if got != u32::from(expect) {
            return Err(format!(
                "atoms {key:0width$b}: formula is {expect}, but the CNF has {got} satisfying extensions",
                width = atoms.len()
            ));
        }
    }
    Ok(())
}

/// Encoders that are checked against the formula. [`Mode::Honest`] is a
/// second copy of the Tseitin encoding, so the harness is checked too. The
/// others each break one rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Honest,
    /// An `or` gate encoded as an `and` gate.
    OrAsAnd,
    /// An `and` gate without its clause `all literals -> output`.
    AndWithoutBack,
    /// The unit clause for the root is not added.
    DropRootUnit,
}

/// A Tseitin encoder written again for the tests. It takes the same atoms
/// and produces a [`Base`] that the checker can use. Its only difference
/// from the library is the rule chosen by [`Mode`].
struct Mirror<'a> {
    arena: &'a Arena,
    mode: Mode,
    clauses: Vec<Vec<Lit>>,
    next_var: u32,
    memo: BTreeMap<u32, Lit>,
    atom_var: BTreeMap<u32, u32>,
}

impl Mirror<'_> {
    fn fresh(&mut self) -> Lit {
        let v = self.next_var;
        self.next_var += 1;
        Lit::new(v, false)
    }

    fn lits(&mut self, gs: &[Formula]) -> Vec<Lit> {
        gs.iter().map(|&g| self.lit(g)).collect()
    }

    fn lit(&mut self, f: Formula) -> Lit {
        if let Some(&l) = self.memo.get(&f.0) {
            return l;
        }
        let arena = self.arena;
        let lit = match arena.view(f) {
            NodeView::Const(b) => Lit::new(0, !b),
            NodeView::Atom {
                terms,
                bound,
                strict,
            } => {
                if terms.is_empty() {
                    Lit::new(0, !constant_atom(bound, strict))
                } else {
                    let v = self.fresh();
                    self.atom_var.insert(f.0, v.var() as u32);
                    v
                }
            }
            NodeView::Not(g) => self.lit(g).negate(),
            NodeView::And(gs) => {
                let ls = self.lits(gs);
                self.and_gate(&ls)
            }
            NodeView::Or(gs) => {
                let ls = self.lits(gs);
                if self.mode == Mode::OrAsAnd {
                    self.and_gate(&ls)
                } else {
                    self.or_gate(&ls)
                }
            }
        };
        self.memo.insert(f.0, lit);
        lit
    }

    fn and_gate(&mut self, ls: &[Lit]) -> Lit {
        if ls.is_empty() {
            return Lit::new(0, false);
        }
        if ls.len() == 1 {
            return ls[0];
        }
        let x = self.fresh();
        for &l in ls {
            self.clauses.push(vec![x.negate(), l]);
        }
        if self.mode != Mode::AndWithoutBack {
            let mut big: Vec<Lit> = ls.iter().map(|l| l.negate()).collect();
            big.push(x);
            self.clauses.push(big);
        }
        x
    }

    fn or_gate(&mut self, ls: &[Lit]) -> Lit {
        if ls.is_empty() {
            return Lit::new(0, true);
        }
        if ls.len() == 1 {
            return ls[0];
        }
        let x = self.fresh();
        let mut big: Vec<Lit> = ls.to_vec();
        big.push(x.negate());
        self.clauses.push(big);
        for &l in ls {
            self.clauses.push(vec![l.negate(), x]);
        }
        x
    }
}

/// The base that a [`Mirror`] in `mode` produces for the single root `root`.
fn mirror_base(arena: &Arena, root: Formula, mode: Mode) -> Base {
    let mut m = Mirror {
        arena,
        mode,
        clauses: vec![vec![Lit::new(0, false)]],
        next_var: 1,
        memo: BTreeMap::new(),
        atom_var: BTreeMap::new(),
    };
    let r = m.lit(root);
    if mode != Mode::DropRootUnit {
        m.clauses.push(vec![r]);
    }
    let mut clauses = m.clauses;
    for c in &mut clauses {
        c.sort_unstable_by_key(|l| l.0);
        c.dedup();
    }
    Base {
        clauses,
        n_vars: m.next_var as usize,
        rows: Vec::new(),
        atoms: Vec::new(),
        atom_var: m.atom_var,
    }
}

fn honest_base(arena: &Arena, root: Formula) -> Base {
    Base::new(arena, &[root], usize::MAX).unwrap()
}

/// Records which node kinds occur under `f`.
fn kinds(arena: &Arena, f: Formula, out: &mut BTreeSet<&'static str>) {
    match arena.view(f) {
        NodeView::Const(_) => {
            out.insert("constant");
        }
        NodeView::Atom { terms, .. } => {
            out.insert(if terms.is_empty() {
                "constant atom"
            } else {
                "linear atom"
            });
        }
        NodeView::Not(g) => {
            out.insert("not");
            kinds(arena, g, out);
        }
        NodeView::And(gs) => connective(arena, gs, ["and", "empty and", "unary and"], out),
        NodeView::Or(gs) => connective(arena, gs, ["or", "empty or", "unary or"], out),
    }
}

/// `labels` are the plain, empty and one-child names for this connective.
fn connective(
    arena: &Arena,
    gs: &[Formula],
    labels: [&'static str; 3],
    out: &mut BTreeSet<&'static str>,
) {
    out.insert(labels[0]);
    match gs.len() {
        0 => {
            out.insert(labels[1]);
        }
        1 => {
            out.insert(labels[2]);
        }
        _ => {}
    }
    for (i, g) in gs.iter().enumerate() {
        if gs[..i].contains(g) {
            out.insert("repeated child");
        }
        kinds(arena, *g, out);
    }
}

/// Random Boolean formulas over two real variables, drawn from every
/// connective and some degenerate shapes: constants, constant atoms, empty
/// and one-child `and`/`or`, double negation, repeated children, and `implies`
/// (which the arena builds as `or(not a, b)`).
fn gen_formula(rng: &mut Rng, arena: &mut Arena, vars: [Var; 2], depth: u32) -> Formula {
    if depth == 0 || rng.range(0, 4) == 0 {
        return match rng.range(0, 7) {
            0 => arena.constant(rng.range(0, 1) == 1),
            1 => {
                let a = Linear::constant(Rational::from_int(rng.range(-2, 2) as i128));
                let b = Linear::constant(Rational::from_int(rng.range(-2, 2) as i128));
                match rng.range(0, 3) {
                    0 => arena.le(&a, &b),
                    1 => arena.lt(&a, &b),
                    2 => arena.ge(&a, &b),
                    _ => arena.gt(&a, &b),
                }
                .unwrap()
            }
            _ => build_arena(arena, vars, &random_atom(rng)),
        };
    }
    match rng.range(0, 7) {
        0..=3 => {
            let n = if rng.range(0, 7) == 0 {
                0
            } else {
                rng.range(1, 3) as usize
            };
            // Sometimes every child is the same formula, which shares its nodes.
            let kids: Vec<Formula> = if rng.range(0, 3) == 0 && n > 0 {
                vec![gen_formula(rng, arena, vars, depth - 1); n]
            } else {
                (0..n)
                    .map(|_| gen_formula(rng, arena, vars, depth - 1))
                    .collect()
            };
            if rng.range(0, 1) == 0 {
                arena.and(kids)
            } else {
                arena.or(kids)
            }
        }
        4 => {
            let g = gen_formula(rng, arena, vars, depth - 1);
            arena.not(g)
        }
        5 => {
            let a = gen_formula(rng, arena, vars, depth - 1);
            let b = gen_formula(rng, arena, vars, depth - 1);
            arena.implies(a, b)
        }
        _ => {
            let g = gen_formula(rng, arena, vars, depth - 1);
            let h = arena.not(g);
            arena.not(h)
        }
    }
}

/// Random formulas whose honest CNF has at most [`MAX_VARS`] variables.
/// Returns the formulas and how many were too large.
fn small_corpus(seed: u64, cases: usize, depth: u32) -> (Vec<(Arena, Formula)>, usize) {
    let mut rng = Rng(seed);
    let mut out = Vec::new();
    let mut skipped = 0;
    for _ in 0..cases {
        let mut arena = Arena::new();
        let x = arena.declare("x");
        let y = arena.declare("y");
        let f = gen_formula(&mut rng, &mut arena, [x, y], depth);
        if honest_base(&arena, f).n_vars <= MAX_VARS {
            out.push((arena, f));
        } else {
            skipped += 1;
        }
    }
    (out, skipped)
}

const REQUIRED_KINDS: [&str; 11] = [
    "constant",
    "constant atom",
    "linear atom",
    "not",
    "and",
    "or",
    "empty and",
    "empty or",
    "unary and",
    "unary or",
    "repeated child",
];

#[test]
fn honest_encoding_matches_formula_on_random_formulas() {
    let (corpus, skipped) = small_corpus(0x7513_0000_ABCD_0001, 3000, 4);
    assert!(
        corpus.len() >= 500,
        "only {} checked, {skipped} too large",
        corpus.len()
    );
    let mut seen = BTreeSet::new();
    for (case, (arena, f)) in corpus.iter().enumerate() {
        let base = honest_base(arena, *f);
        if let Err(e) = encodes_exactly(arena, *f, &base) {
            panic!("case {case}: {e}");
        }
        kinds(arena, *f, &mut seen);
    }
    for k in REQUIRED_KINDS {
        assert!(seen.contains(k), "no formula with a {k}");
    }
}

#[test]
fn honest_encoding_matches_formula_on_edge_cases() {
    let mut arena = Arena::new();
    let x = arena.declare("x");
    let y = arena.declare("y");
    let a = arena
        .le(&Linear::var(x), &Linear::constant(Rational::from_int(0)))
        .unwrap();
    let b = arena
        .ge(&Linear::var(y), &Linear::constant(Rational::from_int(1)))
        .unwrap();
    let c = arena.lt(&Linear::var(x), &Linear::var(y)).unwrap();
    let t = arena.constant(true);
    let fa = arena.constant(false);
    let cases = [
        t,
        fa,
        arena.and(vec![]),
        arena.or(vec![]),
        arena.and(vec![a]),
        arena.or(vec![a]),
        arena.and(vec![t, a]),
        arena.or(vec![fa, a]),
        arena.and(vec![a, a]),
        arena.not(a),
        {
            let na = arena.not(a);
            arena.not(na)
        },
        arena.implies(a, b),
        arena.implies(fa, a),
        {
            let ab = arena.and(vec![a, b]);
            let na = arena.not(a);
            let nac = arena.and(vec![na, c]);
            arena.or(vec![ab, nac])
        },
    ];
    for (i, &f) in cases.iter().enumerate() {
        let base = honest_base(&arena, f);
        if let Err(e) = encodes_exactly(&arena, f, &base) {
            panic!("edge case {i}: {e}");
        }
    }
}

#[test]
fn broken_encoders_fail_the_equivalence_check() {
    let (corpus, _) = small_corpus(0x0BAD_C0DE_0000_0002, 600, 4);
    assert!(corpus.len() >= 200, "only {} cases", corpus.len());
    for mode in [
        Mode::Honest,
        Mode::OrAsAnd,
        Mode::AndWithoutBack,
        Mode::DropRootUnit,
    ] {
        let failures = corpus
            .iter()
            .filter(|(arena, f)| {
                let base = mirror_base(arena, *f, mode);
                encodes_exactly(arena, *f, &base).is_err()
            })
            .count();
        if mode == Mode::Honest {
            assert_eq!(failures, 0, "the control encoder is wrong");
        } else {
            assert!(failures > 0, "{mode:?} was not detected");
        }
    }
}

#[test]
fn proof_built_on_broken_or_gate_is_rejected() {
    // `x <= 0 or x >= 10` is satisfiable, so no proof of its unsatisfiability
    // exists. A broken encoder that makes `or` behave as `and` makes the
    // formula look refutable: the root then forces both atoms, and the theory
    // lemma `not (x <= 0 and x >= 10)` closes the gap.
    let mut arena = Arena::new();
    let x = arena.declare("x");
    let le0 = arena
        .le(&Linear::var(x), &Linear::constant(Rational::from_int(0)))
        .unwrap();
    let ge10 = arena
        .ge(&Linear::var(x), &Linear::constant(Rational::from_int(10)))
        .unwrap();
    let f = arena.or(vec![le0, ge10]);

    let lits = vec![(le0, true), (ge10, true)];
    let cert = farkas_conflict(&arena, &lits, u64::MAX)
        .unwrap()
        .expect("the two atoms are contradictory");
    let proof = Proof::new(vec![
        ProofStep::Theory { lits, cert },
        ProofStep::Rup(Vec::new()),
    ]);

    let honest = honest_base(&arena, f);
    let broken = mirror_base(&arena, f, Mode::OrAsAnd);
    assert!(
        encodes_exactly(&arena, f, &honest).is_ok(),
        "the honest encoding must match"
    );
    assert!(
        encodes_exactly(&arena, f, &broken).is_err(),
        "the broken encoding must fail the equivalence check"
    );
    assert!(
        check_steps(&broken, &arena, &proof),
        "the proof should check against the broken base, which is what makes it a threat"
    );
    assert!(
        !check_proof(&arena, &[f], &proof),
        "the checker must reject the proof against the honest encoding"
    );
}

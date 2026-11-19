//! A conflict-driven clause learning (CDCL) SAT solver with a theory hook.
//!
//! Propagation uses two watched literals per clause. Conflicts are analysed
//! to the first unique implication point, and the learned clause is added.
//! When every Boolean variable is assigned, the theory checks the assignment.
//! If the theory rejects it, the returned clause blocks the assignment, and
//! the search continues.

use alloc::vec;
use alloc::vec::Vec;

use crate::Unknown;

/// A literal: variable `v` with sign, encoded as `2*v` (positive) or `2*v + 1` (negative).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Lit(pub(crate) u32);

impl Lit {
    pub(crate) fn new(var: u32, negative: bool) -> Self {
        Lit(var * 2 + u32::from(negative))
    }

    pub(crate) fn var(self) -> usize {
        (self.0 >> 1) as usize
    }

    pub(crate) fn is_negative(self) -> bool {
        self.0 & 1 == 1
    }

    pub(crate) fn negate(self) -> Self {
        Lit(self.0 ^ 1)
    }
}

/// Result of a satisfiability check.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SatResult {
    Sat,
    Unsat,
}

/// The theory's answer on a full Boolean assignment.
pub(crate) type TheoryCheck<'a> = &'a mut dyn FnMut(&[i8]) -> Result<Option<Vec<Lit>>, Unknown>;

/// A CDCL solver that keeps its clause database and learned clauses between
/// calls to [`Sat::solve`]. Clauses added between calls are simplified against
/// the level-0 assignment. Assumptions are decided first and are not clauses,
/// so anything learned while they were assumed stays valid after they are dropped.
pub(crate) struct Sat {
    n_vars: usize,
    clauses: Vec<Vec<Lit>>,
    watches: Vec<Vec<usize>>,
    /// Per variable: 0 unassigned, 1 true, -1 false.
    assign: Vec<i8>,
    level: Vec<usize>,
    reason: Vec<Option<usize>>,
    activity: Vec<u64>,
    bump: u64,
    trail: Vec<Lit>,
    trail_lim: Vec<usize>,
    qhead: usize,
    /// Set when the clauses (with no assumptions) are unsatisfiable. Permanent.
    theory_unsat: bool,
    /// Clause count above which `solve` returns `Unknown`.
    max_clauses: usize,
}

impl Sat {
    /// An empty solver with no variables.
    pub(crate) const fn new() -> Self {
        Self {
            n_vars: 0,
            clauses: Vec::new(),
            watches: Vec::new(),
            assign: Vec::new(),
            level: Vec::new(),
            reason: Vec::new(),
            activity: Vec::new(),
            bump: 1,
            trail: Vec::new(),
            trail_lim: Vec::new(),
            qhead: 0,
            theory_unsat: false,
            max_clauses: usize::MAX,
        }
    }

    /// Makes variables `0..n` available.
    pub(crate) fn ensure_vars(&mut self, n: usize) {
        if n <= self.n_vars {
            return;
        }
        self.n_vars = n;
        self.watches.resize_with(2 * n, Vec::new);
        self.assign.resize(n, 0);
        self.level.resize(n, 0);
        self.reason.resize(n, None);
        self.activity.resize(n, 0);
    }

    /// Sets the clause limit. Going over it makes `solve` return `Unknown`.
    pub(crate) fn set_max_clauses(&mut self, max: usize) {
        self.max_clauses = max;
    }

    /// Adds a clause. Backtracks to level 0 first, and simplifies the clause
    /// against the level-0 assignment.
    pub(crate) fn add_clause(&mut self, mut lits: Vec<Lit>) {
        self.backtrack(0);
        if self.theory_unsat {
            return;
        }
        lits.sort_unstable_by_key(|l| l.0);
        lits.dedup();
        // A clause with both x and not x is always true.
        if lits.windows(2).any(|w| w[0].negate() == w[1]) {
            return;
        }
        let mut kept = Vec::with_capacity(lits.len());
        for &l in &lits {
            match self.value(l) {
                1 => return,
                -1 => {}
                _ => kept.push(l),
            }
        }
        match kept.len() {
            0 => self.theory_unsat = true,
            1 => self.enqueue(kept[0], None),
            _ => {
                self.attach(kept);
            }
        }
    }

    fn value(&self, lit: Lit) -> i8 {
        let v = self.assign[lit.var()];
        if lit.is_negative() { -v } else { v }
    }

    fn decision_level(&self) -> usize {
        self.trail_lim.len()
    }

    fn enqueue(&mut self, lit: Lit, reason: Option<usize>) {
        let v = lit.var();
        self.assign[v] = if lit.is_negative() { -1 } else { 1 };
        self.level[v] = self.decision_level();
        self.reason[v] = reason;
        self.trail.push(lit);
    }

    /// Decides whether the clauses and `assumptions` (all true) are satisfiable.
    ///
    /// `theory` is called on each full Boolean assignment (one value per
    /// variable: 1, -1), and returns a clause to learn, or `None` if the
    /// assignment is consistent. Clauses learned here stay in the database.
    ///
    /// `Unsat` may mean the assumptions alone are inconsistent with the
    /// clauses. Only an `Unsat` with no assumptions involved is permanent.
    pub(crate) fn solve(
        &mut self,
        assumptions: &[Lit],
        theory: TheoryCheck<'_>,
    ) -> Result<SatResult, Unknown> {
        if self.theory_unsat {
            return Ok(SatResult::Unsat);
        }
        self.backtrack(0);
        if self.propagate().is_some() {
            self.theory_unsat = true;
            return Ok(SatResult::Unsat);
        }
        loop {
            if self.clauses.len() > self.max_clauses {
                return Err(Unknown);
            }
            if let Some(confl) = self.propagate() {
                if self.decision_level() == 0 {
                    self.theory_unsat = true;
                    return Ok(SatResult::Unsat);
                }
                self.resolve_conflict(confl);
                continue;
            }
            let level = self.decision_level();
            if level < assumptions.len() {
                let a = assumptions[level];
                match self.value(a) {
                    // Already true: an empty level keeps the level/assumption correspondence.
                    1 => self.trail_lim.push(self.trail.len()),
                    -1 => {
                        self.backtrack(0);
                        return Ok(SatResult::Unsat);
                    }
                    _ => {
                        self.trail_lim.push(self.trail.len());
                        self.enqueue(a, None);
                    }
                }
                continue;
            }
            if let Some(var) = self.pick_branch() {
                self.trail_lim.push(self.trail.len());
                self.enqueue(Lit::new(var as u32, true), None);
                continue;
            }
            // Every variable is assigned. Ask the theory.
            match theory(&self.assign)? {
                None => return Ok(SatResult::Sat),
                Some(clause) => match self.learn_theory_clause(clause) {
                    Some(ci) => self.resolve_conflict(ci),
                    None => {
                        // A unit clause was asserted at level 0, or the problem is unsat.
                        if self.theory_unsat {
                            return Ok(SatResult::Unsat);
                        }
                    }
                },
            }
        }
    }

    /// Adds a clause with at least two literals to the database, watching its first two.
    fn attach(&mut self, lits: Vec<Lit>) -> usize {
        let ci = self.clauses.len();
        self.watches[lits[0].0 as usize].push(ci);
        self.watches[lits[1].0 as usize].push(ci);
        self.clauses.push(lits);
        ci
    }

    /// Adds a clause that is false under the current full assignment.
    ///
    /// Returns the index of the clause to analyse. Returns `None` if the clause
    /// was handled without a conflict (a unit clause asserted at level 0), or
    /// if the problem is unsatisfiable, which sets `theory_unsat`.
    fn learn_theory_clause(&mut self, mut lits: Vec<Lit>) -> Option<usize> {
        lits.sort_unstable_by_key(|l| l.0);
        lits.dedup();
        if lits.is_empty() {
            self.theory_unsat = true;
            return None;
        }
        if lits.len() == 1 {
            let l = lits[0];
            self.backtrack(0);
            return match self.value(l) {
                0 => {
                    self.enqueue(l, None);
                    None
                }
                _ => {
                    self.theory_unsat = true;
                    None
                }
            };
        }
        // Watch the two literals assigned at the highest levels.
        lits.sort_by_key(|l| core::cmp::Reverse(self.level[l.var()]));
        let top = self.level[lits[0].var()];
        if top == 0 {
            // Every literal is false at level 0, so the problem is unsat.
            self.theory_unsat = true;
            return None;
        }
        // Backtrack to the top level so that the clause has a literal at the current level.
        self.backtrack(top);
        Some(self.attach(lits))
    }

    /// Handles a conflicting clause: learns the first-UIP clause and backjumps.
    fn resolve_conflict(&mut self, confl: usize) {
        let (learnt, back_level) = self.analyze(confl);
        self.backtrack(back_level);
        if learnt.len() == 1 {
            self.enqueue(learnt[0], None);
        } else {
            let asserting = learnt[0];
            let ci = self.attach(learnt);
            self.enqueue(asserting, Some(ci));
        }
        self.bump_all();
    }

    /// First-UIP conflict analysis. Returns the learned clause, with the
    /// asserting literal first, and the level to backjump to.
    fn analyze(&mut self, confl: usize) -> (Vec<Lit>, usize) {
        let current = self.decision_level();
        let mut seen = vec![false; self.n_vars];
        let mut learnt: Vec<Lit> = vec![Lit(0)];
        let mut open = 0usize;
        let mut idx = self.trail.len();
        let mut clause = confl;
        let mut resolved: Option<Lit> = None;
        loop {
            for k in 0..self.clauses[clause].len() {
                let q = self.clauses[clause][k];
                if resolved.is_some_and(|p| p.var() == q.var()) {
                    continue;
                }
                let v = q.var();
                if seen[v] || self.level[v] == 0 {
                    continue;
                }
                seen[v] = true;
                self.activity[v] = self.activity[v].saturating_add(self.bump);
                if self.level[v] >= current {
                    open += 1;
                } else {
                    learnt.push(q);
                }
            }
            // Walk back along the trail to the next literal that needs resolving.
            loop {
                idx -= 1;
                if seen[self.trail[idx].var()] {
                    break;
                }
            }
            let p = self.trail[idx];
            seen[p.var()] = false;
            open -= 1;
            if open == 0 {
                learnt[0] = p.negate();
                break;
            }
            clause = match self.reason[p.var()] {
                Some(c) => c,
                None => {
                    learnt[0] = p.negate();
                    break;
                }
            };
            resolved = Some(p);
        }
        // Put the literal from the highest remaining level second, so it can be watched.
        let mut back = 0;
        if learnt.len() > 1 {
            let mut best = 1;
            for i in 2..learnt.len() {
                if self.level[learnt[i].var()] > self.level[learnt[best].var()] {
                    best = i;
                }
            }
            learnt.swap(1, best);
            back = self.level[learnt[1].var()];
        }
        (learnt, back)
    }

    fn backtrack(&mut self, level: usize) {
        if self.decision_level() <= level {
            return;
        }
        let keep = self.trail_lim[level];
        for i in (keep..self.trail.len()).rev() {
            let v = self.trail[i].var();
            self.assign[v] = 0;
            self.reason[v] = None;
        }
        self.trail.truncate(keep);
        self.trail_lim.truncate(level);
        self.qhead = keep;
    }

    /// Two-watched-literal unit propagation. Returns a conflicting clause index, if any.
    fn propagate(&mut self) -> Option<usize> {
        while self.qhead < self.trail.len() {
            let p = self.trail[self.qhead];
            self.qhead += 1;
            let false_lit = p.negate();
            let mut ws = core::mem::take(&mut self.watches[false_lit.0 as usize]);
            let mut i = 0;
            let mut j = 0;
            let mut conflict = None;
            while i < ws.len() {
                let ci = ws[i];
                i += 1;
                if self.clauses[ci][0] == false_lit {
                    self.clauses[ci].swap(0, 1);
                }
                if self.value(self.clauses[ci][0]) == 1 {
                    ws[j] = ci;
                    j += 1;
                    continue;
                }
                let len = self.clauses[ci].len();
                let mut moved = false;
                for k in 2..len {
                    let cand = self.clauses[ci][k];
                    if self.value(cand) != -1 {
                        self.clauses[ci].swap(1, k);
                        self.watches[cand.0 as usize].push(ci);
                        moved = true;
                        break;
                    }
                }
                if moved {
                    continue;
                }
                ws[j] = ci;
                j += 1;
                let first = self.clauses[ci][0];
                if self.value(first) == -1 {
                    conflict = Some(ci);
                    while i < ws.len() {
                        ws[j] = ws[i];
                        j += 1;
                        i += 1;
                    }
                } else {
                    self.enqueue(first, Some(ci));
                }
            }
            ws.truncate(j);
            self.watches[false_lit.0 as usize] = ws;
            if conflict.is_some() {
                return conflict;
            }
        }
        None
    }

    /// The unassigned variable with the highest activity.
    fn pick_branch(&self) -> Option<usize> {
        let mut best: Option<usize> = None;
        for v in 0..self.n_vars {
            if self.assign[v] == 0 && best.is_none_or(|b| self.activity[v] > self.activity[b]) {
                best = Some(v);
            }
        }
        best
    }

    fn bump_all(&mut self) {
        self.bump = self.bump.saturating_add(self.bump / 16 + 1);
        if self.bump > u64::MAX / 4 {
            // Rescale to avoid overflow.
            for a in &mut self.activity {
                *a >>= 8;
            }
            self.bump >>= 8;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn sat_with(n: usize) -> Sat {
        let mut s = Sat::new();
        s.ensure_vars(n);
        s
    }

    fn no_theory(_: &[i8]) -> Result<Option<Vec<Lit>>, Unknown> {
        Ok(None)
    }

    fn lit(v: u32, neg: bool) -> Lit {
        Lit::new(v, neg)
    }

    #[test]
    fn satisfiable_instance() {
        // (a or b) and (not a or b) and (not b or a)  => a = b = true
        let mut s = sat_with(2);
        s.add_clause(vec![lit(0, false), lit(1, false)]);
        s.add_clause(vec![lit(0, true), lit(1, false)]);
        s.add_clause(vec![lit(1, true), lit(0, false)]);
        let mut th = no_theory;
        assert_eq!(s.solve(&[], &mut th).unwrap(), SatResult::Sat);
        assert_eq!(s.assign, vec![1, 1]);
    }

    #[test]
    fn unsatisfiable_instance() {
        // All four clauses over two variables: unsat.
        let mut s = sat_with(2);
        for &(a, b) in &[(false, false), (false, true), (true, false), (true, true)] {
            s.add_clause(vec![lit(0, a), lit(1, b)]);
        }
        let mut th = no_theory;
        assert_eq!(s.solve(&[], &mut th).unwrap(), SatResult::Unsat);
    }

    #[test]
    fn pigeonhole_three_into_two_is_unsat() {
        // p(i,h): pigeon i in hole h. Variables 0..6: p(i,h) = 2*i + h.
        let mut s = sat_with(6);
        for i in 0..3 {
            s.add_clause(vec![lit(2 * i, false), lit(2 * i + 1, false)]);
        }
        for h in 0..2 {
            for i in 0..3 {
                for j in (i + 1)..3 {
                    s.add_clause(vec![lit(2 * i + h, true), lit(2 * j + h, true)]);
                }
            }
        }
        let mut th = no_theory;
        assert_eq!(s.solve(&[], &mut th).unwrap(), SatResult::Unsat);
    }

    #[test]
    fn theory_clause_is_learned() {
        // Boolean part is free. The theory forbids the assignment (a=1, b=1).
        let mut s = sat_with(2);
        let mut calls = 0;
        let mut th = |a: &[i8]| {
            calls += 1;
            if a[0] == 1 && a[1] == 1 {
                Ok(Some(vec![lit(0, true), lit(1, true)]))
            } else {
                Ok(None)
            }
        };
        assert_eq!(s.solve(&[], &mut th).unwrap(), SatResult::Sat);
        assert!(calls >= 1);
    }
}

//! General simplex for linear real arithmetic, after Dutertre and de Moura.
//!
//! Each atom's expression is a slack variable `s = sum(c_i x_i)`, and the atom
//! becomes a bound on `s`. Bounds are delta-rationals `a + b*delta`, which
//! handle strict inequalities. Bland's rule makes the search terminate.
//!
//! When the bounds cannot be met, the check returns the set of bounds that
//! conflict. Each bound carries the SAT literal that asserted it, so the
//! caller can learn a clause that blocks the conflict.

use alloc::vec;
use alloc::vec::Vec;

use crate::Unknown;
use crate::rational::Rational;

/// A value `real + infinitesimal * delta`, compared lexicographically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DRat {
    pub(crate) real: Rational,
    pub(crate) delta: Rational,
}

impl DRat {
    pub(crate) const ZERO: Self = Self {
        real: Rational::ZERO,
        delta: Rational::ZERO,
    };

    pub(crate) fn new(real: Rational, delta: Rational) -> Self {
        Self { real, delta }
    }

    pub(crate) fn checked_add(self, o: Self) -> Option<Self> {
        Some(Self {
            real: self.real.checked_add(o.real)?,
            delta: self.delta.checked_add(o.delta)?,
        })
    }

    pub(crate) fn checked_sub(self, o: Self) -> Option<Self> {
        Some(Self {
            real: self.real.checked_sub(o.real)?,
            delta: self.delta.checked_sub(o.delta)?,
        })
    }

    pub(crate) fn scale(self, k: Rational) -> Option<Self> {
        Some(Self {
            real: self.real.checked_mul(k)?,
            delta: self.delta.checked_mul(k)?,
        })
    }
}

impl PartialOrd for DRat {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DRat {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.real
            .cmp(&other.real)
            .then(self.delta.cmp(&other.delta))
    }
}

/// A bound on a variable, with the literal that asserted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bound {
    pub(crate) value: DRat,
    pub(crate) reason: u32,
}

/// Outcome of [`Simplex::check`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Check {
    /// All bounds hold. Values can be read with [`Simplex::value`].
    Sat,
    /// The bounds conflict. Each entry is the reason of a bound involved and
    /// its Farkas coefficient. The coefficients are positive, and the
    /// weighted bounds sum to a contradiction (see [`crate::Farkas`]).
    Conflict(Vec<(u32, Rational)>),
}

/// A simplex tableau over `n` original variables and `m` slack variables.
///
/// Variables `0..n` are the original reals. Variable `n + j` is slack `j`,
/// which equals the linear combination given in `rows[j]`.
pub(crate) struct Simplex {
    n_total: usize,
    /// `tableau[r][k]`: coefficient of nonbasic variable `k` in the row of basic `basic[r]`.
    tableau: Vec<Vec<Rational>>,
    basic: Vec<usize>,
    /// Row index of each basic variable, or `None` if nonbasic.
    row_of: Vec<Option<usize>>,
    value: Vec<DRat>,
    lower: Vec<Option<Bound>>,
    upper: Vec<Option<Bound>>,
    /// Pivots made so far, and the most this instance may make.
    pub(crate) pivots: u64,
    pivot_limit: u64,
}

impl Simplex {
    /// Builds the tableau. `rows[j]` gives slack `n + j` as a combination of
    /// originals. Pivoting past `pivot_limit` returns `Unknown`.
    pub(crate) fn new(
        n: usize,
        rows: &[Vec<(usize, Rational)>],
        pivot_limit: u64,
    ) -> Result<Self, Unknown> {
        let m = rows.len();
        let n_total = n + m;
        let mut tableau = vec![vec![Rational::ZERO; n_total]; m];
        let mut basic = Vec::with_capacity(m);
        let mut row_of = vec![None; n_total];
        for (j, row) in rows.iter().enumerate() {
            for &(i, c) in row {
                tableau[j][i] = c;
            }
            basic.push(n + j);
            row_of[n + j] = Some(j);
        }
        Ok(Self {
            n_total,
            tableau,
            basic,
            row_of,
            value: vec![DRat::ZERO; n_total],
            lower: vec![None; n_total],
            upper: vec![None; n_total],
            pivots: 0,
            pivot_limit,
        })
    }

    /// Removes every bound. The tableau and the current values are kept, so
    /// the next check starts from the basis that the last one ended with.
    pub(crate) fn reset_bounds(&mut self) {
        self.lower.iter_mut().for_each(|b| *b = None);
        self.upper.iter_mut().for_each(|b| *b = None);
    }

    /// Starts a new pivot budget: no pivots counted, and at most `limit` allowed.
    pub(crate) fn start_budget(&mut self, limit: u64) {
        self.pivots = 0;
        self.pivot_limit = limit;
    }

    /// Asserts `x <= bound`.
    pub(crate) fn assert_upper(
        &mut self,
        x: usize,
        bound: DRat,
        reason: u32,
    ) -> Result<Check, Unknown> {
        if self.upper[x].is_some_and(|cur| cur.value <= bound) {
            return Ok(Check::Sat);
        }
        if let Some(lo) = self.lower[x]
            && bound < lo.value
        {
            return Ok(Check::Conflict(vec![
                (reason, Rational::ONE),
                (lo.reason, Rational::ONE),
            ]));
        }
        self.upper[x] = Some(Bound {
            value: bound,
            reason,
        });
        if self.row_of[x].is_none() && self.value[x] > bound {
            self.update(x, bound)?;
        }
        Ok(Check::Sat)
    }

    /// Asserts `x >= bound`.
    pub(crate) fn assert_lower(
        &mut self,
        x: usize,
        bound: DRat,
        reason: u32,
    ) -> Result<Check, Unknown> {
        if self.lower[x].is_some_and(|cur| cur.value >= bound) {
            return Ok(Check::Sat);
        }
        if let Some(hi) = self.upper[x]
            && bound > hi.value
        {
            return Ok(Check::Conflict(vec![
                (reason, Rational::ONE),
                (hi.reason, Rational::ONE),
            ]));
        }
        self.lower[x] = Some(Bound {
            value: bound,
            reason,
        });
        if self.row_of[x].is_none() && self.value[x] < bound {
            self.update(x, bound)?;
        }
        Ok(Check::Sat)
    }

    /// Restores all bounds, or reports a conflict.
    pub(crate) fn check(&mut self) -> Result<Check, Unknown> {
        loop {
            // Bland's rule: the basic variable with the smallest index that violates a bound.
            let mut pick: Option<usize> = None;
            for r in 0..self.basic.len() {
                let b = self.basic[r];
                if self.violates(b) && pick.is_none_or(|p| b < self.basic[p]) {
                    pick = Some(r);
                }
            }
            let Some(r) = pick else {
                return Ok(Check::Sat);
            };
            let b = self.basic[r];
            let increase = self.lower[b].is_some_and(|lo| self.value[b] < lo.value);

            // Find the smallest nonbasic variable that can move in the needed direction.
            let mut entering: Option<usize> = None;
            for k in 0..self.n_total {
                let c = self.tableau[r][k];
                if c.is_zero() || self.row_of[k].is_some() {
                    continue;
                }
                // Moving x_k up raises b when c > 0, and moving it down raises b when c < 0.
                let up_helps = c.is_negative() != increase;
                let can_move = if up_helps {
                    self.upper[k].is_none_or(|hi| self.value[k] < hi.value)
                } else {
                    self.lower[k].is_none_or(|lo| self.value[k] > lo.value)
                };
                if can_move {
                    entering = Some(k);
                    break;
                }
            }

            match entering {
                Some(k) => {
                    let target = if increase {
                        self.lower[b].map(|x| x.value)
                    } else {
                        self.upper[b].map(|x| x.value)
                    };
                    let Some(target) = target else {
                        return Ok(Check::Sat);
                    };
                    self.pivot_and_update(r, k, target)?;
                }
                None => return Ok(Check::Conflict(self.explain(r, increase))),
            }
        }
    }

    fn violates(&self, x: usize) -> bool {
        self.lower[x].is_some_and(|lo| self.value[x] < lo.value)
            || self.upper[x].is_some_and(|hi| self.value[x] > hi.value)
    }

    /// Sets nonbasic `x` to `v` and updates the basic variables that depend on it.
    fn update(&mut self, x: usize, v: DRat) -> Result<(), Unknown> {
        let delta = v.checked_sub(self.value[x]).ok_or(Unknown)?;
        for r in 0..self.basic.len() {
            let c = self.tableau[r][x];
            if c.is_zero() {
                continue;
            }
            let b = self.basic[r];
            self.value[b] = self.value[b]
                .checked_add(delta.scale(c).ok_or(Unknown)?)
                .ok_or(Unknown)?;
        }
        self.value[x] = v;
        Ok(())
    }

    /// Makes basic `basic[r]` take value `target` by moving nonbasic `k`, then swaps them.
    fn pivot_and_update(&mut self, r: usize, k: usize, target: DRat) -> Result<(), Unknown> {
        let b = self.basic[r];
        let a = self.tableau[r][k];
        let theta = target
            .checked_sub(self.value[b])
            .and_then(|d| d.scale(Rational::ONE.checked_div(a)?))
            .ok_or(Unknown)?;
        self.value[b] = target;
        self.value[k] = self.value[k].checked_add(theta).ok_or(Unknown)?;
        for r2 in 0..self.basic.len() {
            if r2 == r {
                continue;
            }
            let c = self.tableau[r2][k];
            if c.is_zero() {
                continue;
            }
            let bb = self.basic[r2];
            self.value[bb] = self.value[bb]
                .checked_add(theta.scale(c).ok_or(Unknown)?)
                .ok_or(Unknown)?;
        }
        self.pivot(r, k)
    }

    /// Exchanges basic `basic[r]` with nonbasic `k` and rewrites the tableau.
    fn pivot(&mut self, r: usize, k: usize) -> Result<(), Unknown> {
        self.pivots += 1;
        if self.pivots > self.pivot_limit {
            return Err(Unknown);
        }
        let b = self.basic[r];
        let inv = Rational::ONE
            .checked_div(self.tableau[r][k])
            .ok_or(Unknown)?;
        // New row for k: k = (b - sum_{j != k} T[r][j] x_j) / T[r][k].
        let mut new_row = vec![Rational::ZERO; self.n_total];
        for j in 0..self.n_total {
            if j == k {
                continue;
            }
            new_row[j] = if j == b {
                inv
            } else {
                self.tableau[r][j]
                    .checked_mul(inv)
                    .ok_or(Unknown)?
                    .checked_neg()
                    .ok_or(Unknown)?
            };
        }
        for r2 in 0..self.basic.len() {
            if r2 == r {
                continue;
            }
            let c = self.tableau[r2][k];
            if c.is_zero() {
                continue;
            }
            self.tableau[r2][k] = Rational::ZERO;
            for j in 0..self.n_total {
                if new_row[j].is_zero() {
                    continue;
                }
                let add = c.checked_mul(new_row[j]).ok_or(Unknown)?;
                self.tableau[r2][j] = self.tableau[r2][j].checked_add(add).ok_or(Unknown)?;
            }
        }
        self.tableau[r] = new_row;
        self.basic[r] = k;
        self.row_of[k] = Some(r);
        self.row_of[b] = None;
        Ok(())
    }

    /// Reasons for the infeasible row `r`: its violated bound, and the bounds
    /// that stop the nonbasic variables from moving.
    ///
    /// The Farkas coefficients are 1 for the violated bound of `b`, and `|c|`
    /// for the blocking bound of each nonbasic `k` with tableau entry `c`.
    /// Writing `b = sum c_k x_k`, these weighted bounds cancel the variables
    /// and leave a contradiction.
    fn explain(&self, r: usize, increase: bool) -> Vec<(u32, Rational)> {
        let b = self.basic[r];
        let mut reasons = Vec::new();
        let own = if increase {
            self.lower[b]
        } else {
            self.upper[b]
        };
        reasons.extend(own.map(|x| (x.reason, Rational::ONE)));
        for k in 0..self.n_total {
            let c = self.tableau[r][k];
            if c.is_zero() || self.row_of[k].is_some() {
                continue;
            }
            // Blocked upward when c > 0 (for increase) and so on.
            let blocked_up = c.is_negative() != increase;
            let bound = if blocked_up {
                self.upper[k]
            } else {
                self.lower[k]
            };
            let weight = if c.is_negative() {
                c.checked_neg().unwrap_or(c)
            } else {
                c
            };
            reasons.extend(bound.map(|x| (x.reason, weight)));
        }
        reasons
    }

    /// Concrete values for all variables, after [`Simplex::check`] returned `Sat`.
    ///
    /// Strict bounds leave `delta` symbolic. This picks a positive rational
    /// `delta` small enough that every bound still holds with it.
    pub(crate) fn concrete_values(&self) -> Option<Vec<Rational>> {
        let mut delta = Rational::ONE;
        let tighten = |delta: &mut Rational,
                       lo_real: Rational,
                       lo_d: Rational,
                       hi_real: Rational,
                       hi_d: Rational|
         -> Option<()> {
            // Need lo_real + lo_d*d <= hi_real + hi_d*d, with d > 0.
            if lo_real < hi_real && lo_d > hi_d {
                let cand = hi_real
                    .checked_sub(lo_real)?
                    .checked_div(lo_d.checked_sub(hi_d)?)?;
                if cand < *delta {
                    *delta = cand;
                }
            }
            Some(())
        };
        for x in 0..self.n_total {
            let v = self.value[x];
            if let Some(lo) = self.lower[x] {
                tighten(&mut delta, lo.value.real, lo.value.delta, v.real, v.delta)?;
            }
            if let Some(hi) = self.upper[x] {
                tighten(&mut delta, v.real, v.delta, hi.value.real, hi.value.delta)?;
            }
        }
        let mut out = Vec::with_capacity(self.n_total);
        for &v in &self.value {
            out.push(v.real.checked_add(v.delta.checked_mul(delta)?)?);
        }
        Some(out)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn r(n: i128) -> Rational {
        Rational::from_int(n)
    }

    fn le(v: i128) -> DRat {
        DRat::new(r(v), Rational::ZERO)
    }

    #[test]
    fn feasible_bounds_are_met() {
        // s = x + y, with x <= 2, y <= 3, s >= 4.
        let rows = vec![vec![(0, r(1)), (1, r(1))]];
        let mut s = Simplex::new(2, &rows, u64::MAX).unwrap();
        assert_eq!(s.assert_upper(0, le(2), 1).unwrap(), Check::Sat);
        assert_eq!(s.assert_upper(1, le(3), 2).unwrap(), Check::Sat);
        assert_eq!(s.assert_lower(2, le(4), 3).unwrap(), Check::Sat);
        assert_eq!(s.check().unwrap(), Check::Sat);
        let vals = s.concrete_values().unwrap();
        assert!(
            vals[0] <= r(2) && vals[1] <= r(3) && vals[0].checked_add(vals[1]).unwrap() >= r(4)
        );
    }

    #[test]
    fn conflicting_bounds_explain_themselves() {
        // s = x + y, with x <= 1, y <= 1, s >= 5: reasons 1, 2 and 3 all matter.
        let rows = vec![vec![(0, r(1)), (1, r(1))]];
        let mut s = Simplex::new(2, &rows, u64::MAX).unwrap();
        s.assert_upper(0, le(1), 1).unwrap();
        s.assert_upper(1, le(1), 2).unwrap();
        s.assert_lower(2, le(5), 3).unwrap();
        match s.check().unwrap() {
            Check::Conflict(reasons) => {
                let mut reasons: Vec<u32> = reasons.into_iter().map(|(r, _)| r).collect();
                reasons.sort_unstable();
                reasons.dedup();
                assert_eq!(reasons, vec![1, 2, 3]);
            }
            Check::Sat => panic!("expected conflict"),
        }
    }

    #[test]
    fn strict_bound_uses_delta() {
        // x > 0 and x < 1: needs a positive delta.
        let rows: Vec<Vec<(usize, Rational)>> = Vec::new();
        let mut s = Simplex::new(1, &rows, u64::MAX).unwrap();
        assert_eq!(
            s.assert_lower(0, DRat::new(r(0), r(1)), 1).unwrap(),
            Check::Sat
        );
        assert_eq!(
            s.assert_upper(0, DRat::new(r(1), r(-1)), 2).unwrap(),
            Check::Sat
        );
        assert_eq!(s.check().unwrap(), Check::Sat);
        let v = s.concrete_values().unwrap()[0];
        assert!(v > r(0) && v < r(1));
    }

    #[test]
    fn strict_conflict_detected() {
        // x < 1 and x >= 1 conflict.
        let rows: Vec<Vec<(usize, Rational)>> = Vec::new();
        let mut s = Simplex::new(1, &rows, u64::MAX).unwrap();
        s.assert_upper(0, DRat::new(r(1), r(-1)), 1).unwrap();
        let out = s.assert_lower(0, le(1), 2).unwrap();
        assert!(matches!(out, Check::Conflict(_)));
    }
}

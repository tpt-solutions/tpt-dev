//! Linear expressions over real variables, and an arena of Boolean formulas
//! whose atoms are linear inequalities.
//!
//! Every atom has the form `sum(c_i * x_i) <= k` or `sum(c_i * x_i) < k`.
//! Equalities and `>=`/`>` are rewritten into these forms when they are built.

use alloc::string::String;
use alloc::vec::Vec;

use crate::rational::Rational;

/// A real-valued variable. Created by [`Arena::declare`] or by the solver.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Var(pub(crate) u32);

impl Var {
    /// The variable's index in declaration order.
    pub const fn index(&self) -> usize {
        self.0 as usize
    }
}

/// A linear expression `sum(c_i * x_i) + constant`.
///
/// Terms are kept sorted by variable, with no zero coefficients.
///
/// ```
/// use tpt_dev_solver::{Arena, Linear, Rational};
///
/// let mut arena = Arena::new();
/// let x = arena.declare("x");
/// let e = Linear::var(x).scale(Rational::from_int(2)).unwrap()
///     .checked_add(&Linear::constant(Rational::from_int(1))).unwrap();
/// assert_eq!(e.constant_term(), Rational::ONE);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Linear {
    terms: Vec<(Var, Rational)>,
    constant: Rational,
}

impl Linear {
    /// The constant expression `c`.
    pub fn constant(c: Rational) -> Self {
        Self {
            terms: Vec::new(),
            constant: c,
        }
    }

    /// The expression `x`.
    pub fn var(x: Var) -> Self {
        Self {
            terms: alloc::vec![(x, Rational::ONE)],
            constant: Rational::ZERO,
        }
    }

    /// The variable terms, sorted by variable.
    pub fn terms(&self) -> &[(Var, Rational)] {
        &self.terms
    }

    /// The constant part.
    pub const fn constant_term(&self) -> Rational {
        self.constant
    }

    /// `self + other`, or `None` on overflow.
    pub fn checked_add(&self, other: &Self) -> Option<Self> {
        let mut terms: Vec<(Var, Rational)> =
            Vec::with_capacity(self.terms.len() + other.terms.len());
        let (mut i, mut j) = (0, 0);
        while i < self.terms.len() || j < other.terms.len() {
            match (self.terms.get(i), other.terms.get(j)) {
                (Some(&(v1, c1)), Some(&(v2, c2))) if v1 == v2 => {
                    let c = c1.checked_add(c2)?;
                    if !c.is_zero() {
                        terms.push((v1, c));
                    }
                    i += 1;
                    j += 1;
                }
                (Some(&(v1, c1)), Some(&(v2, _))) if v1 < v2 => {
                    terms.push((v1, c1));
                    i += 1;
                }
                (Some(_), Some(&(v2, c2))) => {
                    terms.push((v2, c2));
                    j += 1;
                }
                (Some(&(v1, c1)), None) => {
                    terms.push((v1, c1));
                    i += 1;
                }
                (None, Some(&(v2, c2))) => {
                    terms.push((v2, c2));
                    j += 1;
                }
                (None, None) => break,
            }
        }
        Some(Self {
            terms,
            constant: self.constant.checked_add(other.constant)?,
        })
    }

    /// `self - other`, or `None` on overflow.
    pub fn checked_sub(&self, other: &Self) -> Option<Self> {
        self.checked_add(&other.scale(Rational::from_int(-1))?)
    }

    /// `k * self`, or `None` on overflow.
    pub fn scale(&self, k: Rational) -> Option<Self> {
        if k.is_zero() {
            return Some(Self::constant(Rational::ZERO));
        }
        let mut terms = Vec::with_capacity(self.terms.len());
        for &(v, c) in &self.terms {
            terms.push((v, c.checked_mul(k)?));
        }
        Some(Self {
            terms,
            constant: self.constant.checked_mul(k)?,
        })
    }
}

/// An atom `sum(terms) < bound` (when `strict`) or `sum(terms) <= bound`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Atom {
    pub(crate) terms: Vec<(Var, Rational)>,
    pub(crate) bound: Rational,
    pub(crate) strict: bool,
}

/// Identifies a formula in an [`Arena`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Formula(pub(crate) u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Node {
    Const(bool),
    Atom(Atom),
    Not(Formula),
    And(Vec<Formula>),
    Or(Vec<Formula>),
}

/// Owns variable names and formula nodes. Nodes are never removed, so a
/// [`Formula`] stays valid for the arena's lifetime.
///
/// ```
/// use tpt_dev_solver::{Arena, Linear, Rational};
///
/// let mut arena = Arena::new();
/// let x = arena.declare("x");
/// // x <= 3
/// let f = arena.le(&Linear::var(x), &Linear::constant(Rational::from_int(3))).unwrap();
/// assert_eq!(arena.var_name(x), "x");
/// # let _ = f;
/// ```
pub struct Arena {
    names: Vec<String>,
    nodes: Vec<Node>,
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Arena {
    /// Creates an empty arena.
    pub const fn new() -> Self {
        Self {
            names: Vec::new(),
            nodes: Vec::new(),
        }
    }

    /// Declares a new real variable called `name`. Two declarations with the
    /// same name give two different variables.
    pub fn declare(&mut self, name: &str) -> Var {
        let v = Var(self.names.len() as u32);
        self.names.push(String::from(name));
        v
    }

    /// The name a variable was declared with.
    pub fn var_name(&self, v: Var) -> &str {
        self.names.get(v.index()).map_or("", |s| s.as_str())
    }

    /// Number of declared variables.
    pub fn num_vars(&self) -> usize {
        self.names.len()
    }

    /// The constant formula `true` or `false`.
    pub fn constant(&mut self, value: bool) -> Formula {
        self.push(Node::Const(value))
    }

    /// `a <= b`.
    pub fn le(&mut self, a: &Linear, b: &Linear) -> Option<Formula> {
        self.compare(a, b, false)
    }

    /// `a < b`.
    pub fn lt(&mut self, a: &Linear, b: &Linear) -> Option<Formula> {
        self.compare(a, b, true)
    }

    /// `a >= b`.
    pub fn ge(&mut self, a: &Linear, b: &Linear) -> Option<Formula> {
        self.compare(b, a, false)
    }

    /// `a > b`.
    pub fn gt(&mut self, a: &Linear, b: &Linear) -> Option<Formula> {
        self.compare(b, a, true)
    }

    /// `a = b`, as `a <= b and a >= b`.
    pub fn eq(&mut self, a: &Linear, b: &Linear) -> Option<Formula> {
        let le = self.le(a, b)?;
        let ge = self.ge(a, b)?;
        Some(self.and(alloc::vec![le, ge]))
    }

    /// Negation.
    pub fn not(&mut self, f: Formula) -> Formula {
        self.push(Node::Not(f))
    }

    /// Conjunction. An empty list gives `true`.
    pub fn and(&mut self, fs: Vec<Formula>) -> Formula {
        self.push(Node::And(fs))
    }

    /// Disjunction. An empty list gives `false`.
    pub fn or(&mut self, fs: Vec<Formula>) -> Formula {
        self.push(Node::Or(fs))
    }

    /// `a => b`, as `not a or b`.
    pub fn implies(&mut self, a: Formula, b: Formula) -> Formula {
        let na = self.not(a);
        self.or(alloc::vec![na, b])
    }

    fn compare(&mut self, a: &Linear, b: &Linear, strict: bool) -> Option<Formula> {
        // a - b <op> 0, so the variable part bounds the negated constant.
        let diff = a.checked_sub(b)?;
        let bound = diff.constant.checked_neg()?;
        Some(self.push(Node::Atom(Atom {
            terms: diff.terms,
            bound,
            strict,
        })))
    }

    fn push(&mut self, node: Node) -> Formula {
        let id = Formula(self.nodes.len() as u32);
        self.nodes.push(node);
        id
    }

    pub(crate) fn node(&self, f: Formula) -> &Node {
        // Formulas are only created by this arena, so the index is in range.
        &self.nodes[f.0 as usize]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn int(n: i128) -> Rational {
        Rational::from_int(n)
    }

    #[test]
    fn linear_add_merges_terms() {
        let mut a = Arena::new();
        let x = a.declare("x");
        let y = a.declare("y");
        let e1 = Linear::var(y).checked_add(&Linear::var(x)).unwrap();
        let e2 = Linear::var(x).scale(int(-1)).unwrap();
        let sum = e1.checked_add(&e2).unwrap();
        assert_eq!(sum.terms(), &[(y, Rational::ONE)]);
    }

    #[test]
    fn compare_moves_constants_to_bound() {
        let mut a = Arena::new();
        let x = a.declare("x");
        // x + 2 <= 5  is  x <= 3
        let lhs = Linear::var(x)
            .checked_add(&Linear::constant(int(2)))
            .unwrap();
        let f = a.le(&lhs, &Linear::constant(int(5))).unwrap();
        match a.node(f) {
            Node::Atom(atom) => {
                assert_eq!(atom.bound, int(3));
                assert!(!atom.strict);
                assert_eq!(atom.terms, alloc::vec![(x, Rational::ONE)]);
            }
            other => panic!("expected atom, got {other:?}"),
        }
    }
}

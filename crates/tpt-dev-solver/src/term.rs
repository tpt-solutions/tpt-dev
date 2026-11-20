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
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Arena;
    ///
    /// let mut arena = Arena::new();
    /// let x = arena.declare("x");
    /// let y = arena.declare("y");
    /// assert_eq!((x.index(), y.index()), (0, 1));
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational};
    ///
    /// let e = Linear::constant(Rational::from_int(3));
    /// assert!(e.terms().is_empty());
    /// assert_eq!(e.constant_term(), Rational::from_int(3));
    /// ```
    pub fn constant(c: Rational) -> Self {
        Self {
            terms: Vec::new(),
            constant: c,
        }
    }

    /// The expression `x`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Arena, Linear, Rational};
    ///
    /// let mut arena = Arena::new();
    /// let x = arena.declare("x");
    /// let e = Linear::var(x);
    /// assert_eq!(e.terms(), &[(x, Rational::ONE)]);
    /// ```
    pub fn var(x: Var) -> Self {
        Self {
            terms: alloc::vec![(x, Rational::ONE)],
            constant: Rational::ZERO,
        }
    }

    /// The variable terms, sorted by variable.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Arena, Linear, Rational};
    ///
    /// let mut arena = Arena::new();
    /// let x = arena.declare("x");
    /// assert_eq!(Linear::var(x).terms(), &[(x, Rational::ONE)]);
    /// ```
    pub fn terms(&self) -> &[(Var, Rational)] {
        &self.terms
    }

    /// The constant part.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational};
    ///
    /// assert_eq!(Linear::constant(Rational::from_int(2)).constant_term(), Rational::from_int(2));
    /// ```
    pub const fn constant_term(&self) -> Rational {
        self.constant
    }

    /// `self + other`, or `None` on overflow.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Arena, Linear, Rational};
    ///
    /// let mut arena = Arena::new();
    /// let x = arena.declare("x");
    /// let sum = Linear::var(x).checked_add(&Linear::constant(Rational::ONE)).unwrap();
    /// assert_eq!(sum.constant_term(), Rational::ONE);
    /// assert_eq!(sum.terms().len(), 1);
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Arena, Linear};
    ///
    /// let mut arena = Arena::new();
    /// let x = arena.declare("x");
    /// // Terms that cancel are dropped, so the result has no variables.
    /// let zero = Linear::var(x).checked_sub(&Linear::var(x)).unwrap();
    /// assert!(zero.terms().is_empty());
    /// ```
    pub fn checked_sub(&self, other: &Self) -> Option<Self> {
        self.checked_add(&other.scale(Rational::from_int(-1))?)
    }

    /// `k * self`, or `None` on overflow.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Arena, Linear, Rational};
    ///
    /// let mut arena = Arena::new();
    /// let x = arena.declare("x");
    /// let tripled = Linear::var(x).scale(Rational::from_int(3)).unwrap();
    /// assert_eq!(tripled.terms(), &[(x, Rational::from_int(3))]);
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Arena;
    ///
    /// let arena = Arena::new();
    /// assert_eq!(arena.num_vars(), 0);
    /// ```
    pub const fn new() -> Self {
        Self {
            names: Vec::new(),
            nodes: Vec::new(),
        }
    }

    /// Declares a new real variable called `name`. Two declarations with the
    /// same name give two different variables.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Arena;
    ///
    /// let mut arena = Arena::new();
    /// let x = arena.declare("x");
    /// assert_eq!(arena.var_name(x), "x");
    /// ```
    pub fn declare(&mut self, name: &str) -> Var {
        let v = Var(self.names.len() as u32);
        self.names.push(String::from(name));
        v
    }

    /// The name a variable was declared with.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Arena;
    ///
    /// let mut arena = Arena::new();
    /// let x = arena.declare("speed");
    /// assert_eq!(arena.var_name(x), "speed");
    /// ```
    pub fn var_name(&self, v: Var) -> &str {
        self.names.get(v.index()).map_or("", |s| s.as_str())
    }

    /// Number of declared variables.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::Arena;
    ///
    /// let mut arena = Arena::new();
    /// arena.declare("x");
    /// arena.declare("y");
    /// assert_eq!(arena.num_vars(), 2);
    /// ```
    pub fn num_vars(&self) -> usize {
        self.names.len()
    }

    /// The constant formula `true` or `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Arena, Solver, Status};
    ///
    /// let mut arena = Arena::new();
    /// let t = arena.constant(true);
    /// let mut solver = Solver::new();
    /// *solver.arena_mut() = arena;
    /// solver.assert(t);
    /// assert_eq!(solver.check(), Ok(Status::Sat));
    /// ```
    pub fn constant(&mut self, value: bool) -> Formula {
        self.push(Node::Const(value))
    }

    /// `a <= b`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let zero = Linear::constant(Rational::ZERO);
    /// // x <= 0 together with x > 0 is unsatisfiable.
    /// let le = solver.arena_mut().le(&Linear::var(x), &zero).unwrap();
    /// let gt = solver.arena_mut().gt(&Linear::var(x), &zero).unwrap();
    /// solver.assert(le);
    /// solver.assert(gt);
    /// assert_eq!(solver.check(), Ok(Status::Unsat));
    /// ```
    pub fn le(&mut self, a: &Linear, b: &Linear) -> Option<Formula> {
        self.compare(a, b, false)
    }

    /// `a < b`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let zero = Linear::constant(Rational::ZERO);
    /// // x < 0 together with x >= 0 is unsatisfiable.
    /// let lt = solver.arena_mut().lt(&Linear::var(x), &zero).unwrap();
    /// let ge = solver.arena_mut().ge(&Linear::var(x), &zero).unwrap();
    /// solver.assert(lt);
    /// solver.assert(ge);
    /// assert_eq!(solver.check(), Ok(Status::Unsat));
    /// ```
    pub fn lt(&mut self, a: &Linear, b: &Linear) -> Option<Formula> {
        self.compare(a, b, true)
    }

    /// `a >= b`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let one = Linear::constant(Rational::ONE);
    /// // x >= 1 together with x <= 0 is unsatisfiable.
    /// let ge = solver.arena_mut().ge(&Linear::var(x), &one).unwrap();
    /// let le = solver.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
    /// solver.assert(ge);
    /// solver.assert(le);
    /// assert_eq!(solver.check(), Ok(Status::Unsat));
    /// ```
    pub fn ge(&mut self, a: &Linear, b: &Linear) -> Option<Formula> {
        self.compare(b, a, false)
    }

    /// `a > b`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let zero = Linear::constant(Rational::ZERO);
    /// // x > 0 together with x <= 0 is unsatisfiable.
    /// let gt = solver.arena_mut().gt(&Linear::var(x), &zero).unwrap();
    /// let le = solver.arena_mut().le(&Linear::var(x), &zero).unwrap();
    /// solver.assert(gt);
    /// solver.assert(le);
    /// assert_eq!(solver.check(), Ok(Status::Unsat));
    /// ```
    pub fn gt(&mut self, a: &Linear, b: &Linear) -> Option<Formula> {
        self.compare(b, a, true)
    }

    /// `a = b`, as `a <= b and a >= b`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let two = Linear::constant(Rational::from_int(2));
    /// let eq = solver.arena_mut().eq(&Linear::var(x), &two).unwrap();
    /// solver.assert(eq);
    /// assert_eq!(solver.check(), Ok(Status::Sat));
    /// assert_eq!(solver.value(x), Some(Rational::from_int(2)));
    /// ```
    pub fn eq(&mut self, a: &Linear, b: &Linear) -> Option<Formula> {
        let le = self.le(a, b)?;
        let ge = self.ge(a, b)?;
        Some(self.and(alloc::vec![le, ge]))
    }

    /// Negation.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let le = solver.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
    /// let not_le = solver.arena_mut().not(le);
    /// // A formula and its negation cannot both hold.
    /// solver.assert(le);
    /// solver.assert(not_le);
    /// assert_eq!(solver.check(), Ok(Status::Unsat));
    /// ```
    pub fn not(&mut self, f: Formula) -> Formula {
        self.push(Node::Not(f))
    }

    /// Conjunction. An empty list gives `true`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let ge = solver.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
    /// let le = solver.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
    /// let both = solver.arena_mut().and(vec![ge, le]);
    /// solver.assert(both);
    /// assert_eq!(solver.check(), Ok(Status::Unsat));
    /// ```
    pub fn and(&mut self, fs: Vec<Formula>) -> Formula {
        self.push(Node::And(fs))
    }

    /// Disjunction. An empty list gives `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let ge = solver.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
    /// let le = solver.arena_mut().le(&Linear::var(x), &Linear::constant(Rational::from_int(-1))).unwrap();
    /// let either = solver.arena_mut().or(vec![ge, le]);
    /// solver.assert(either);
    /// // x = 0 satisfies neither disjunct.
    /// let zero = solver.arena_mut().eq(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
    /// solver.assert(zero);
    /// assert_eq!(solver.check(), Ok(Status::Unsat));
    /// ```
    pub fn or(&mut self, fs: Vec<Formula>) -> Formula {
        self.push(Node::Or(fs))
    }

    /// `a => b`, as `not a or b`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Linear, Rational, Solver, Status};
    ///
    /// let mut solver = Solver::new();
    /// let x = solver.declare("x");
    /// let a = solver.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::ONE)).unwrap();
    /// let b = solver.arena_mut().ge(&Linear::var(x), &Linear::constant(Rational::ZERO)).unwrap();
    /// let rule = solver.arena_mut().implies(a, b);
    /// // With a true, the rule forces b; so not-b together with a is unsatisfiable.
    /// let not_b = solver.arena_mut().not(b);
    /// solver.assert(rule);
    /// solver.assert(a);
    /// solver.assert(not_b);
    /// assert_eq!(solver.check(), Ok(Status::Unsat));
    /// ```
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

/// What the encoder needs to see of one formula node. Borrowed from the store,
/// so reading a node never allocates.
pub(crate) enum NodeView<'a> {
    Const(bool),
    Atom {
        terms: &'a [(Var, Rational)],
        bound: Rational,
        strict: bool,
    },
    Not(Formula),
    And(&'a [Formula]),
    Or(&'a [Formula]),
}

/// A store of formulas that the solver can read. Implemented by [`Arena`]
/// (heap-backed) and [`FixedArena`] (inline storage).
pub(crate) trait Source {
    /// Number of declared real variables.
    fn var_count(&self) -> usize;
    /// The node behind `f`. `f` must come from this store.
    fn view(&self, f: Formula) -> NodeView<'_>;
}

impl Source for Arena {
    fn var_count(&self) -> usize {
        self.names.len()
    }

    fn view(&self, f: Formula) -> NodeView<'_> {
        match self.node(f) {
            Node::Const(b) => NodeView::Const(*b),
            Node::Atom(a) => NodeView::Atom {
                terms: &a.terms,
                bound: a.bound,
                strict: a.strict,
            },
            Node::Not(g) => NodeView::Not(*g),
            Node::And(gs) => NodeView::And(gs),
            Node::Or(gs) => NodeView::Or(gs),
        }
    }
}

/// A node in a [`FixedArena`]. Ranges index the arena's term and child pools.
#[derive(Clone, Copy, Debug)]
enum FixedNode {
    Const(bool),
    Atom {
        start: usize,
        len: usize,
        bound: Rational,
        strict: bool,
    },
    Not(Formula),
    And {
        start: usize,
        len: usize,
    },
    Or {
        start: usize,
        len: usize,
    },
}

/// A formula arena with inline storage and a fixed capacity, for use where
/// the formula store must not allocate.
///
/// The const parameters are the capacities: `VARS` variable names, `NODES`
/// formula nodes, `TERMS` atom coefficients in total, and `KIDS` children of
/// `and`/`or` nodes in total. Every builder returns `None` when its pool is
/// full, and never grows.
///
/// The store itself makes no heap allocation. Solving still uses `alloc`
/// for the search state (see [`FixedArena::check`]).
///
/// ```
/// use tpt_dev_solver::{Budget, FixedArena, Rational, Status};
///
/// let mut arena: FixedArena<2, 8, 8, 8> = FixedArena::new();
/// let x = arena.declare("x").unwrap();
/// // x >= 1 and x <= 3, written as -x <= -1 and x <= 3
/// let lo = arena.atom(&[(x, Rational::from_int(-1))], Rational::from_int(-1), false).unwrap();
/// let hi = arena.atom(&[(x, Rational::ONE)], Rational::from_int(3), false).unwrap();
/// assert_eq!(arena.check(&[lo, hi], &Budget::UNLIMITED), Ok(Status::Sat));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct FixedArena<const VARS: usize, const NODES: usize, const TERMS: usize, const KIDS: usize>
{
    names: [&'static str; VARS],
    n_vars: usize,
    nodes: [FixedNode; NODES],
    n_nodes: usize,
    terms: [(Var, Rational); TERMS],
    n_terms: usize,
    kids: [Formula; KIDS],
    n_kids: usize,
}

impl<const VARS: usize, const NODES: usize, const TERMS: usize, const KIDS: usize>
    FixedArena<VARS, NODES, TERMS, KIDS>
{
    /// Creates an empty arena.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::FixedArena;
    ///
    /// let arena: FixedArena<1, 1, 1, 1> = FixedArena::new();
    /// assert_eq!(arena.num_vars(), 0);
    /// ```
    pub const fn new() -> Self {
        Self {
            names: [""; VARS],
            n_vars: 0,
            nodes: [FixedNode::Const(false); NODES],
            n_nodes: 0,
            terms: [(Var(0), Rational::ZERO); TERMS],
            n_terms: 0,
            kids: [Formula(0); KIDS],
            n_kids: 0,
        }
    }

    /// Declares a real variable. Returns `None` when all `VARS` slots are used.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::FixedArena;
    ///
    /// let mut arena: FixedArena<1, 1, 1, 1> = FixedArena::new();
    /// assert!(arena.declare("x").is_some());
    /// // The arena has one variable slot, so a second declaration fails.
    /// assert!(arena.declare("y").is_none());
    /// ```
    pub fn declare(&mut self, name: &'static str) -> Option<Var> {
        if self.n_vars == VARS {
            return None;
        }
        let v = Var(self.n_vars as u32);
        self.names[self.n_vars] = name;
        self.n_vars += 1;
        Some(v)
    }

    /// The name a variable was declared with.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::FixedArena;
    ///
    /// let mut arena: FixedArena<2, 1, 1, 1> = FixedArena::new();
    /// let x = arena.declare("speed").unwrap();
    /// assert_eq!(arena.var_name(x), "speed");
    /// ```
    pub fn var_name(&self, v: Var) -> &'static str {
        self.names.get(v.index()).copied().unwrap_or("")
    }

    /// Number of declared variables.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::FixedArena;
    ///
    /// let mut arena: FixedArena<2, 1, 1, 1> = FixedArena::new();
    /// arena.declare("x");
    /// assert_eq!(arena.num_vars(), 1);
    /// ```
    pub fn num_vars(&self) -> usize {
        self.n_vars
    }

    /// Number of formula nodes built so far.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::FixedArena;
    ///
    /// let mut arena: FixedArena<1, 4, 1, 1> = FixedArena::new();
    /// arena.constant(true);
    /// assert_eq!(arena.num_nodes(), 1);
    /// ```
    pub fn num_nodes(&self) -> usize {
        self.n_nodes
    }

    /// The constant `true` or `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Budget, FixedArena, Status};
    ///
    /// let mut arena: FixedArena<1, 2, 1, 1> = FixedArena::new();
    /// let t = arena.constant(true).unwrap();
    /// assert_eq!(arena.check(&[t], &Budget::UNLIMITED), Ok(Status::Sat));
    /// let f = arena.constant(false).unwrap();
    /// assert_eq!(arena.check(&[f], &Budget::UNLIMITED), Ok(Status::Unsat));
    /// ```
    pub fn constant(&mut self, value: bool) -> Option<Formula> {
        self.push(FixedNode::Const(value))
    }

    /// The atom `sum(c_i * x_i) < bound` (when `strict`) or `<= bound`.
    ///
    /// `terms` must be sorted by variable, with distinct variables, nonzero
    /// coefficients and variables that were declared here. Otherwise `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Budget, FixedArena, Rational, Status};
    ///
    /// let mut arena: FixedArena<1, 2, 2, 1> = FixedArena::new();
    /// let x = arena.declare("x").unwrap();
    /// // x <= 3
    /// let le3 = arena.atom(&[(x, Rational::ONE)], Rational::from_int(3), false).unwrap();
    /// assert_eq!(arena.check(&[le3], &Budget::UNLIMITED), Ok(Status::Sat));
    /// // Unsorted or undeclared terms are refused.
    /// assert!(arena.atom(&[(x, Rational::ZERO)], Rational::ONE, false).is_none());
    /// ```
    pub fn atom(
        &mut self,
        terms: &[(Var, Rational)],
        bound: Rational,
        strict: bool,
    ) -> Option<Formula> {
        let mut prev: Option<u32> = None;
        for &(v, c) in terms {
            if v.index() >= self.n_vars || c.is_zero() || prev.is_some_and(|p| p >= v.0) {
                return None;
            }
            prev = Some(v.0);
        }
        let start = self.n_terms;
        if start + terms.len() > TERMS {
            return None;
        }
        self.terms[start..start + terms.len()].copy_from_slice(terms);
        self.n_terms += terms.len();
        self.push(FixedNode::Atom {
            start,
            len: terms.len(),
            bound,
            strict,
        })
    }

    /// Negation.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Budget, FixedArena, Status};
    ///
    /// let mut arena: FixedArena<1, 4, 1, 1> = FixedArena::new();
    /// let t = arena.constant(true).unwrap();
    /// let not_t = arena.not(t).unwrap();
    /// assert_eq!(arena.check(&[not_t], &Budget::UNLIMITED), Ok(Status::Unsat));
    /// ```
    pub fn not(&mut self, f: Formula) -> Option<Formula> {
        self.push(FixedNode::Not(f))
    }

    /// Conjunction. An empty slice gives `true`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Budget, FixedArena, Status};
    ///
    /// let mut arena: FixedArena<1, 4, 1, 2> = FixedArena::new();
    /// let t = arena.constant(true).unwrap();
    /// let f = arena.constant(false).unwrap();
    /// let both = arena.and(&[t, f]).unwrap();
    /// assert_eq!(arena.check(&[both], &Budget::UNLIMITED), Ok(Status::Unsat));
    /// ```
    pub fn and(&mut self, fs: &[Formula]) -> Option<Formula> {
        let (start, len) = self.kid_range(fs)?;
        self.push(FixedNode::And { start, len })
    }

    /// Disjunction. An empty slice gives `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_solver::{Budget, FixedArena, Status};
    ///
    /// let mut arena: FixedArena<1, 4, 1, 2> = FixedArena::new();
    /// let t = arena.constant(true).unwrap();
    /// let f = arena.constant(false).unwrap();
    /// let either = arena.or(&[t, f]).unwrap();
    /// assert_eq!(arena.check(&[either], &Budget::UNLIMITED), Ok(Status::Sat));
    /// ```
    pub fn or(&mut self, fs: &[Formula]) -> Option<Formula> {
        let (start, len) = self.kid_range(fs)?;
        self.push(FixedNode::Or { start, len })
    }

    fn kid_range(&mut self, fs: &[Formula]) -> Option<(usize, usize)> {
        let start = self.n_kids;
        if start + fs.len() > KIDS {
            return None;
        }
        self.kids[start..start + fs.len()].copy_from_slice(fs);
        self.n_kids += fs.len();
        Some((start, fs.len()))
    }

    fn push(&mut self, node: FixedNode) -> Option<Formula> {
        if self.n_nodes == NODES {
            return None;
        }
        self.nodes[self.n_nodes] = node;
        self.n_nodes += 1;
        Some(Formula((self.n_nodes - 1) as u32))
    }
}

impl<const VARS: usize, const NODES: usize, const TERMS: usize, const KIDS: usize> Default
    for FixedArena<VARS, NODES, TERMS, KIDS>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<const VARS: usize, const NODES: usize, const TERMS: usize, const KIDS: usize> Source
    for FixedArena<VARS, NODES, TERMS, KIDS>
{
    fn var_count(&self) -> usize {
        self.n_vars
    }

    fn view(&self, f: Formula) -> NodeView<'_> {
        match self.nodes[f.0 as usize] {
            FixedNode::Const(b) => NodeView::Const(b),
            FixedNode::Atom {
                start,
                len,
                bound,
                strict,
            } => NodeView::Atom {
                terms: &self.terms[start..start + len],
                bound,
                strict,
            },
            FixedNode::Not(g) => NodeView::Not(g),
            FixedNode::And { start, len } => NodeView::And(&self.kids[start..start + len]),
            FixedNode::Or { start, len } => NodeView::Or(&self.kids[start..start + len]),
        }
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

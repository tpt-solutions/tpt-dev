//! The circuit builder and the tracing front end.
//!
//! A circuit is built by running ordinary Rust code over [`Wire`] values. Each
//! `+` and `-` between wires adds no constraint: it only combines linear
//! expressions. Each `*` between two non-constant wires allocates one internal
//! wire and one constraint `a * b = w`. Constants are free.
//!
//! Generic code written against [`Ring`] runs both natively on `i64` and as a
//! circuit, so tests can compare the two.

use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::ops::{Add, Mul, Sub};

use crate::field::F;

/// The role of a wire. Output wires are public, and so are public inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The constant one, always wire 0.
    One,
    /// Computed inside the circuit.
    Internal,
    /// Supplied by the prover, and private.
    PrivateInput,
    /// Supplied by the prover, and visible to the verifier.
    PublicInput,
    /// A result the verifier sees.
    Output,
}

/// A linear combination of wires: a list of `(wire, coefficient)` pairs, sorted by wire.
pub(crate) type Lc = Vec<(usize, F)>;

pub(crate) struct State {
    /// Witness value of each wire.
    pub(crate) values: Vec<F>,
    /// Role of each wire.
    pub(crate) kinds: Vec<Kind>,
    /// Constraints `a * b = c`, each side a linear combination.
    pub(crate) constraints: Vec<[Lc; 3]>,
}

impl State {
    fn alloc(&mut self, kind: Kind, value: F) -> usize {
        self.values.push(value);
        self.kinds.push(kind);
        self.values.len() - 1
    }

    pub(crate) fn eval(&self, lc: &Lc) -> F {
        lc.iter()
            .fold(F::ZERO, |acc, &(i, c)| acc + c * self.values[i])
    }
}

/// Sorts a linear combination by wire, merges repeats, and drops zero terms.
fn normalize(mut lc: Lc) -> Lc {
    lc.sort_by_key(|&(i, _)| i);
    let mut out: Lc = Vec::with_capacity(lc.len());
    for (i, c) in lc {
        match out.last_mut() {
            Some(last) if last.0 == i => last.1 = last.1 + c,
            _ => out.push((i, c)),
        }
    }
    out.retain(|&(_, c)| c != F::ZERO);
    out
}

/// If the combination is a constant, its value.
fn constant_of(lc: &Lc) -> Option<F> {
    match lc.as_slice() {
        [] => Some(F::ZERO),
        [(0, c)] => Some(*c),
        _ => None,
    }
}

/// Numbers that a generic circuit function can compute with.
///
/// Implemented for `i64`, which runs natively, and for [`Wire`], which records a
/// circuit. `lit` makes a constant in the same context as `self`.
pub trait Ring: Clone + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> {
    /// The constant `c`, made in the same context as `self`.
    fn lit(&self, c: i64) -> Self;
}

impl Ring for i64 {
    fn lit(&self, c: i64) -> i64 {
        c
    }
}

/// A value in a circuit: a linear combination of wires, and its witness value.
#[derive(Clone)]
pub struct Wire<'c> {
    state: &'c RefCell<State>,
    lc: Lc,
    value: F,
}

impl<'c> Wire<'c> {
    fn scaled(self, c: F) -> Wire<'c> {
        let lc = self
            .lc
            .into_iter()
            .map(|(i, k)| (i, k * c))
            .collect::<Vec<_>>();
        Wire {
            state: self.state,
            lc: normalize(lc),
            value: self.value * c,
        }
    }

    /// The witness value, as a signed integer.
    pub fn value(&self) -> i64 {
        self.value.to_i64()
    }
}

impl Add for Wire<'_> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let mut lc = self.lc;
        lc.extend(rhs.lc);
        Wire {
            state: self.state,
            lc: normalize(lc),
            value: self.value + rhs.value,
        }
    }
}

impl Sub for Wire<'_> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        let mut lc = self.lc;
        lc.extend(rhs.lc.into_iter().map(|(i, c)| (i, -c)));
        Wire {
            state: self.state,
            lc: normalize(lc),
            value: self.value - rhs.value,
        }
    }
}

impl Mul for Wire<'_> {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        if let Some(c) = constant_of(&self.lc) {
            return rhs.scaled(c);
        }
        if let Some(c) = constant_of(&rhs.lc) {
            return self.scaled(c);
        }
        let value = self.value * rhs.value;
        let state = self.state;
        let w = {
            let mut s = state.borrow_mut();
            let w = s.alloc(Kind::Internal, value);
            s.constraints.push([self.lc, rhs.lc, vec![(w, F::ONE)]]);
            w
        };
        Wire {
            state,
            lc: vec![(w, F::ONE)],
            value,
        }
    }
}

impl Ring for Wire<'_> {
    fn lit(&self, c: i64) -> Self {
        let f = F::from_i64(c);
        Wire {
            state: self.state,
            lc: normalize(vec![(0, f)]),
            value: f,
        }
    }
}

/// A circuit under construction, with its witness.
pub struct Circuit {
    state: RefCell<State>,
}

impl Default for Circuit {
    fn default() -> Self {
        Self::new()
    }
}

impl Circuit {
    /// An empty circuit, with only the constant one.
    pub fn new() -> Self {
        Self {
            state: RefCell::new(State {
                values: vec![F::ONE],
                kinds: vec![Kind::One],
                constraints: Vec::new(),
            }),
        }
    }

    fn input(&self, kind: Kind, v: i64) -> Wire<'_> {
        let value = F::from_i64(v);
        let i = self.state.borrow_mut().alloc(kind, value);
        Wire {
            state: &self.state,
            lc: vec![(i, F::ONE)],
            value,
        }
    }

    /// A private input, supplied by the prover.
    pub fn private_input(&self, v: i64) -> Wire<'_> {
        self.input(Kind::PrivateInput, v)
    }

    /// A public input, supplied by the prover and visible to the verifier.
    pub fn public_input(&self, v: i64) -> Wire<'_> {
        self.input(Kind::PublicInput, v)
    }

    /// Makes `w` a public output. Returns its index, for [`Circuit::value`].
    pub fn output(&self, w: &Wire<'_>) -> usize {
        let mut s = self.state.borrow_mut();
        let o = s.alloc(Kind::Output, w.value);
        s.constraints
            .push([w.lc.clone(), vec![(0, F::ONE)], vec![(o, F::ONE)]]);
        o
    }

    /// Requires two wires to be equal. The constraint fails the check if they are not.
    pub fn assert_eq(&self, a: &Wire<'_>, b: &Wire<'_>) {
        let diff = (a.clone() - b.clone()).lc;
        self.state
            .borrow_mut()
            .constraints
            .push([diff, vec![(0, F::ONE)], Vec::new()]);
    }

    /// The witness value of wire `index`, as a signed integer.
    pub fn value(&self, index: usize) -> i64 {
        self.state.borrow().values[index].to_i64()
    }

    /// Number of constraints.
    pub fn num_constraints(&self) -> usize {
        self.state.borrow().constraints.len()
    }

    /// Number of wires, including the constant one.
    pub fn num_wires(&self) -> usize {
        self.state.borrow().values.len()
    }

    /// Checks every constraint against the witness. Returns the index of the
    /// first failing constraint.
    pub fn check(&self) -> Result<(), usize> {
        let s = self.state.borrow();
        for (i, [a, b, c]) in s.constraints.iter().enumerate() {
            if s.eval(a) * s.eval(b) != s.eval(c) {
                return Err(i);
            }
        }
        Ok(())
    }

    /// The circuit in the binary `.r1cs` format, for circom-compatible tools.
    pub fn to_r1cs(&self) -> Vec<u8> {
        crate::r1cs::encode(&self.state.borrow())
    }

    /// The witness in the wire order of the `.r1cs` format, for the prover.
    pub fn witness(&self) -> Vec<F> {
        crate::r1cs::ordered_witness(&self.state.borrow())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The function under test. It is written once, and runs natively and as a circuit.
    fn kernel<T: Ring>(x: T, y: T) -> T {
        let t = x.clone() * y.clone() + x.lit(3) * x.clone();
        t - y.lit(7)
    }

    #[test]
    fn the_circuit_agrees_with_native_execution() {
        for (x, y) in [(0, 0), (1, 2), (-4, 9), (12, -3), (-7, -7)] {
            let c = Circuit::new();
            let wx = c.private_input(x);
            let wy = c.public_input(y);
            let out = c.output(&kernel(wx, wy));
            c.check().unwrap();
            assert_eq!(c.value(out), kernel(x, y), "inputs {x}, {y}");
        }
    }

    #[test]
    fn constants_cost_no_constraints() {
        let c = Circuit::new();
        let x = c.private_input(5);
        let _ = x.clone() * x.lit(3) + x.lit(2);
        assert_eq!(c.num_constraints(), 0);
    }

    #[test]
    fn multiplying_two_wires_costs_one_constraint() {
        let c = Circuit::new();
        let x = c.private_input(5);
        let y = c.private_input(6);
        let p = x * y;
        assert_eq!(p.value(), 30);
        assert_eq!(c.num_constraints(), 1);
        c.check().unwrap();
    }

    #[test]
    fn a_false_equality_fails_the_check_at_its_constraint() {
        let c = Circuit::new();
        let a = c.private_input(2);
        let b = c.private_input(3);
        c.assert_eq(&a, &b);
        assert_eq!(c.check(), Err(0));
    }

    #[test]
    fn a_true_equality_passes() {
        let c = Circuit::new();
        let a = c.private_input(2);
        let b = c.private_input(1) + c.private_input(1);
        c.assert_eq(&a, &b);
        c.check().unwrap();
        assert_eq!(c.witness()[0], F::ONE);
    }
}

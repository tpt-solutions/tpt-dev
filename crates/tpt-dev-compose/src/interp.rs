//! Decoding of function bodies and constant expressions for the runtime.
//!
//! A body is decoded once, when its module is instantiated. Block structure is
//! resolved at that point: every `block`, `loop` and `if` knows where its `end`
//! is, and every branch names a label that exists. An instruction outside the
//! supported subset is refused here, so no module runs partly.

use alloc::vec::Vec;

use crate::module::ValType;
use crate::runtime::{Feature, Value};

/// The most locals one function may declare, counting parameters.
pub(crate) const MAX_LOCALS: usize = 50_000;

/// One decoded instruction. Indices are into the function's own index space,
/// the instance's global index space or the local index space, as named.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Op {
    /// `unreachable`: traps.
    Unreachable,
    /// `nop`.
    Nop,
    /// `block`, with the index of its `end` and its result count.
    Block { end: usize, results: usize },
    /// `loop`: a branch to it re-enters it.
    Loop,
    /// `if`, with the index of its `else` (if any), its `end` and its result count.
    If {
        els: Option<usize>,
        end: usize,
        results: usize,
    },
    /// `else`, with the index of the `end` that closes its `if`.
    Else { end: usize },
    /// `end`.
    End,
    /// `br` to the label at this depth.
    Br(usize),
    /// `br_if` to the label at this depth.
    BrIf(usize),
    /// `return`.
    Return,
    /// `call` of the function at this index in the instance's function space.
    Call(usize),
    /// `drop`.
    Drop,
    /// `select`.
    Select,
    /// `local.get`.
    LocalGet(usize),
    /// `local.set`.
    LocalSet(usize),
    /// `local.tee`.
    LocalTee(usize),
    /// `global.get`, at this index in the instance's global index space.
    GlobalGet(usize),
    /// `global.set`, at this index in the instance's global index space.
    GlobalSet(usize),
    /// `i32.const` or `i64.const`.
    Const(Value),
    /// A numeric or conversion instruction, by its opcode byte.
    Numeric(u8),
}

/// A decoded function body.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Body {
    /// The types of the declared locals, after the parameters.
    pub(crate) locals: Vec<ValType>,
    /// The instructions. The last one is the `end` of the function.
    pub(crate) code: Vec<Op>,
}

/// Why a body or expression is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reject {
    /// The code uses something outside the supported subset.
    Unsupported(Feature),
    /// The code is malformed, or uses an index that is out of range.
    Malformed(&'static str),
}

/// What a body may refer to: the module's index spaces, as far as decoding needs them.
pub(crate) struct Scope<'a> {
    /// The number of functions in the instance's function index space.
    pub(crate) funcs: usize,
    /// For each global in the instance's global index space, whether it is mutable.
    pub(crate) globals_mutable: &'a [bool],
}

/// A cursor over the bytes of one body or expression.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn done(&self) -> bool {
        self.pos == self.bytes.len()
    }

    fn byte(&mut self) -> Result<u8, Reject> {
        let b = *self
            .bytes
            .get(self.pos)
            .ok_or(Reject::Malformed("the code ends early"))?;
        self.pos += 1;
        Ok(b)
    }

    fn u32(&mut self) -> Result<u32, Reject> {
        let mut result: u64 = 0;
        for shift in (0..35).step_by(7) {
            let b = self.byte()?;
            result |= u64::from(b & 0x7F) << shift;
            if b & 0x80 == 0 {
                if result > u64::from(u32::MAX) {
                    return Err(Reject::Malformed("an index does not fit in 32 bits"));
                }
                return Ok(result as u32);
            }
        }
        Err(Reject::Malformed("a LEB128 integer is too long"))
    }

    /// Reads a signed LEB128 integer of at most `bits` bits, checking its range.
    fn sleb(&mut self, bits: u32) -> Result<i64, Reject> {
        let max_bytes = bits.div_ceil(7);
        let mut result: i128 = 0;
        let mut shift = 0u32;
        for _ in 0..max_bytes {
            let b = self.byte()?;
            result |= i128::from(b & 0x7F) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                if shift < 128 && b & 0x40 != 0 {
                    result |= -1i128 << shift;
                }
                let min = -(1i128 << (bits - 1));
                let max = (1i128 << (bits - 1)) - 1;
                if result < min || result > max {
                    return Err(Reject::Malformed("a constant does not fit its type"));
                }
                return Ok(result as i64);
            }
        }
        Err(Reject::Malformed("a LEB128 integer is too long"))
    }

    /// Reads a value type that a local or a block may name, refusing the
    /// types the runtime does not support.
    fn value_type(&mut self) -> Result<ValType, Reject> {
        match self.byte()? {
            0x7F => Ok(ValType::I32),
            0x7E => Ok(ValType::I64),
            0x7D | 0x7C | 0x7B | 0x70 | 0x6F => Err(Reject::Unsupported(Feature::NonIntegerType)),
            _ => Err(Reject::Malformed("a value type is not valid")),
        }
    }

    /// Reads a block type and returns its result count. Only the empty type and
    /// one integer result are supported. A type index (multi-value) is refused.
    fn block_results(&mut self) -> Result<usize, Reject> {
        match self.bytes.get(self.pos).copied() {
            Some(0x40) => {
                self.pos += 1;
                Ok(0)
            }
            Some(0x7F | 0x7E) => {
                self.pos += 1;
                Ok(1)
            }
            Some(0x7D | 0x7C | 0x7B | 0x70 | 0x6F) => {
                Err(Reject::Unsupported(Feature::NonIntegerType))
            }
            Some(_) => Err(Reject::Unsupported(Feature::MultiValue)),
            None => Err(Reject::Malformed("the code ends early")),
        }
    }
}

/// Whether an opcode is a numeric or conversion instruction the runtime runs.
fn is_numeric(op: u8) -> bool {
    matches!(
        op,
        0x45..=0x4F | 0x50..=0x5A | 0x67..=0x78 | 0x79..=0x8A | 0xA7 | 0xAC | 0xAD
    )
}

/// Decodes a function body. `params` is the number of parameters, which occupy
/// the first local indices, and `scope` gives the indices the body may name.
pub(crate) fn decode_body(bytes: &[u8], params: usize, scope: &Scope<'_>) -> Result<Body, Reject> {
    let mut r = Reader::new(bytes);

    let groups = r.u32()? as usize;
    let mut locals = Vec::new();
    for _ in 0..groups {
        let count = r.u32()? as usize;
        let ty = r.value_type()?;
        if params + locals.len() + count > MAX_LOCALS {
            return Err(Reject::Malformed("the function declares too many locals"));
        }
        locals.resize(locals.len() + count, ty);
    }
    let local_count = params + locals.len();
    let global_count = scope.globals_mutable.len();

    let mut code: Vec<Op> = Vec::new();
    // Indices of the `block`, `loop` and `if` instructions not yet closed.
    let mut open: Vec<usize> = Vec::new();

    loop {
        let at = code.len();
        let op = r.byte()?;
        let decoded = match op {
            0x00 => Op::Unreachable,
            0x01 => Op::Nop,
            0x02 => {
                let results = r.block_results()?;
                open.push(at);
                Op::Block { end: 0, results }
            }
            0x03 => {
                r.block_results()?;
                open.push(at);
                Op::Loop
            }
            0x04 => {
                let results = r.block_results()?;
                open.push(at);
                Op::If {
                    els: None,
                    end: 0,
                    results,
                }
            }
            0x05 => {
                let &opener = open
                    .last()
                    .ok_or(Reject::Malformed("an else is not inside an if"))?;
                match &mut code[opener] {
                    Op::If { els, .. } if els.is_none() => *els = Some(at),
                    Op::If { .. } => return Err(Reject::Malformed("an if has two elses")),
                    _ => return Err(Reject::Malformed("an else is not inside an if")),
                }
                Op::Else { end: 0 }
            }
            0x0B => {
                code.push(Op::End);
                match open.pop() {
                    None => {
                        if !r.done() {
                            return Err(Reject::Malformed("code follows the end of the function"));
                        }
                        return Ok(Body { locals, code });
                    }
                    Some(opener) => {
                        patch_end(&mut code, opener, at);
                        continue;
                    }
                }
            }
            0x0C | 0x0D => {
                let depth = r.u32()? as usize;
                // The labels are the open blocks and the function itself.
                if depth > open.len() {
                    return Err(Reject::Malformed(
                        "a branch names a label that does not exist",
                    ));
                }
                if op == 0x0C {
                    Op::Br(depth)
                } else {
                    Op::BrIf(depth)
                }
            }
            0x0F => Op::Return,
            0x10 => {
                let f = r.u32()? as usize;
                if f >= scope.funcs {
                    return Err(Reject::Malformed(
                        "a call names a function that does not exist",
                    ));
                }
                Op::Call(f)
            }
            0x1A => Op::Drop,
            0x1B => Op::Select,
            0x20..=0x22 => {
                let i = r.u32()? as usize;
                if i >= local_count {
                    return Err(Reject::Malformed("a local index is out of range"));
                }
                match op {
                    0x20 => Op::LocalGet(i),
                    0x21 => Op::LocalSet(i),
                    _ => Op::LocalTee(i),
                }
            }
            0x23 | 0x24 => {
                let g = r.u32()? as usize;
                if g >= global_count {
                    return Err(Reject::Malformed("a global index is out of range"));
                }
                if op == 0x24 && !scope.globals_mutable[g] {
                    return Err(Reject::Malformed("a global is set but is immutable"));
                }
                if op == 0x23 {
                    Op::GlobalGet(g)
                } else {
                    Op::GlobalSet(g)
                }
            }
            0x41 => Op::Const(Value::I32(r.sleb(32)? as i32)),
            0x42 => Op::Const(Value::I64(r.sleb(64)?)),
            _ if is_numeric(op) => Op::Numeric(op),
            _ => return Err(Reject::Unsupported(Feature::Opcode(op))),
        };
        code.push(decoded);
    }
}

/// Sets the `end` of a block, loop or if, and the `end` of the `else` of an if.
fn patch_end(code: &mut [Op], opener: usize, end: usize) {
    match &mut code[opener] {
        Op::Block { end: e, .. } => *e = end,
        Op::If { els, end: e, .. } => {
            *e = end;
            if let Some(at) = *els {
                code[at] = Op::Else { end };
            }
        }
        _ => {}
    }
}

/// Evaluates a constant expression of the given type.
///
/// Only `i32.const`, `i64.const` and `global.get` of an earlier global are
/// supported. `globals` gives the value of each global so far.
pub(crate) fn const_expr(bytes: &[u8], ty: ValType, globals: &[Value]) -> Result<Value, Reject> {
    let mut r = Reader::new(bytes);
    let value = match r.byte()? {
        0x41 => Value::I32(r.sleb(32)? as i32),
        0x42 => Value::I64(r.sleb(64)?),
        0x23 => {
            let g = r.u32()? as usize;
            *globals.get(g).ok_or(Reject::Malformed(
                "a constant refers to a global that is not yet defined",
            ))?
        }
        other => return Err(Reject::Unsupported(Feature::Opcode(other))),
    };
    if r.byte()? != 0x0B || !r.done() {
        return Err(Reject::Malformed(
            "a constant expression does not end with end",
        ));
    }
    let matches_type = matches!(
        (ty, value),
        (ValType::I32, Value::I32(_)) | (ValType::I64, Value::I64(_))
    );
    if !matches_type {
        return Err(Reject::Malformed(
            "a constant has the wrong type for its global",
        ));
    }
    Ok(value)
}

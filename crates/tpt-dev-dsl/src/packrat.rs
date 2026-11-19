//! A memoised packrat matcher. It gives the same results as the interpreter in
//! `matcher.rs`, including the furthest-failure report. Each rule's result is
//! kept per position, so backtracking does not repeat matching, except where the
//! depth limit makes a kept result unusable and it is matched again.
//!
//! The memo table has a fixed capacity chosen by the caller. When it fills,
//! matching stops with [`MemoFull`]. It never guesses an answer.
//!
//! Two things make a cached result reusable, and both are stored with it:
//!
//! - The failures seen while matching a rule are summarised by the single
//!   failure the interpreter would have kept: the last one at the furthest
//!   position. Replaying that one failure leaves the same state.
//! - The depth limit ([`MAX_DEPTH`]) is the only thing that depends on the
//!   call depth. A result is reused only at a depth where every depth check
//!   inside it gives the same answer as before.

use alloc::vec::Vec;
use core::fmt;

use crate::grammar::{Expr, Grammar};
use crate::matcher::{Expected, MAX_DEPTH, ParseError, Tree};

/// The memo table has no free slot left, so matching stopped with no answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoFull;

impl fmt::Display for MemoFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the packrat memo table is full; use a larger Memo")
    }
}

/// Why a packrat parse produced no tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackratError {
    /// The input does not match. This is the same error the interpreter gives.
    Parse(ParseError),
    /// The memo table filled up before matching finished.
    MemoFull,
}

impl fmt::Display for PackratError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackratError::Parse(e) => e.fmt(f),
            PackratError::MemoFull => MemoFull.fmt(f),
        }
    }
}

/// A memoised result for one rule at one position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    rule: usize,
    pos: usize,
    /// The end of the match, or `None` if the rule failed.
    end: Option<usize>,
    /// The failure the interpreter would have kept from this call alone.
    fail: Option<(usize, Expected)>,
    /// Largest depth, relative to the call, of a depth check that passed.
    ok: usize,
    /// Smallest depth, relative to the call, of a depth check that failed.
    bad: Option<usize>,
}

impl Entry {
    /// Whether this result holds at call depth `depth`: every depth check
    /// inside it must give the same answer again.
    fn valid_at(&self, depth: usize) -> bool {
        depth + self.ok <= MAX_DEPTH && self.bad.is_none_or(|b| depth + b > MAX_DEPTH)
    }
}

/// The memo table for packrat parsing, with room for `N` entries.
///
/// It lives in the caller's memory and is reusable. Call [`Memo::clear`]
/// between inputs, or the entries from the last input will be wrongly
/// reused (they are keyed by rule and position only).
#[derive(Clone, Copy, Debug)]
pub struct Memo<const N: usize> {
    slots: [Option<Entry>; N],
}

impl<const N: usize> Memo<N> {
    /// An empty memo table.
    pub const fn new() -> Self {
        Self { slots: [None; N] }
    }

    /// Forgets every entry, so the table can be used for a new input.
    pub fn clear(&mut self) {
        self.slots = [None; N];
    }
}

impl<const N: usize> Default for Memo<N> {
    fn default() -> Self {
        Self::new()
    }
}

enum Slot {
    Found(usize),
    Vacant(usize),
}

type Fail = Option<(usize, Expected)>;

/// Keeps the interpreter's rule: a failure replaces the current one only if it
/// is at least as far along.
fn apply(current: &mut Fail, event: Fail) {
    if let Some((pos, what)) = event
        && current.is_none_or(|(furthest, _)| pos >= furthest)
    {
        *current = Some((pos, what));
    }
}

fn merge_max(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, None) | (None, x) => x,
    }
}

fn merge_min(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, None) | (None, x) => x,
    }
}

struct Matcher<'g, 'i, 'm, const N: usize> {
    grammar: &'g Grammar,
    input: &'i [u8],
    memo: &'m mut Memo<N>,
    /// Failures from the current call, as the interpreter would keep them.
    furthest: Fail,
    /// Depth checks inside the current call (absolute depths).
    ok_max: Option<usize>,
    bad_min: Option<usize>,
}

impl<const N: usize> Matcher<'_, '_, '_, N> {
    fn fail(&mut self, pos: usize, what: Expected) {
        apply(&mut self.furthest, Some((pos, what)));
    }

    fn hash(rule: usize, pos: usize) -> usize {
        let h = (rule as u32).wrapping_mul(0x9E37_79B9) ^ (pos as u32).wrapping_mul(0x85EB_CA6B);
        (h ^ (h >> 15)) as usize
    }

    /// Finds the slot for `(rule, pos)`: its own slot if present, else the
    /// first free one. Fails if the table has neither.
    fn find(&self, rule: usize, pos: usize) -> Result<Slot, MemoFull> {
        if N == 0 {
            return Err(MemoFull);
        }
        let mut i = Self::hash(rule, pos) % N;
        for _ in 0..N {
            match self.memo.slots[i] {
                None => return Ok(Slot::Vacant(i)),
                Some(e) if e.rule == rule && e.pos == pos => return Ok(Slot::Found(i)),
                Some(_) => i = if i + 1 == N { 0 } else { i + 1 },
            }
        }
        Err(MemoFull)
    }

    fn store(&mut self, entry: Entry) -> Result<(), MemoFull> {
        let i = match self.find(entry.rule, entry.pos)? {
            Slot::Found(i) | Slot::Vacant(i) => i,
        };
        self.memo.slots[i] = Some(entry);
        Ok(())
    }

    /// Matches rule `index` at `pos`, where `depth` is the call depth.
    fn call(&mut self, index: usize, pos: usize, depth: usize) -> Result<Option<usize>, MemoFull> {
        if depth > MAX_DEPTH {
            self.bad_min = merge_min(self.bad_min, Some(depth));
            self.fail(pos, Expected::Recursion);
            return Ok(None);
        }
        if let Slot::Found(i) = self.find(index, pos)?
            && let Some(e) = self.memo.slots[i]
            && e.valid_at(depth)
        {
            apply(&mut self.furthest, e.fail);
            self.ok_max = merge_max(self.ok_max, Some(depth + e.ok));
            self.bad_min = merge_min(self.bad_min, e.bad.map(|b| depth + b));
            return Ok(e.end);
        }

        // Measure this call on its own, then fold the result back into the caller.
        let saved_fail = self.furthest.take();
        let saved_ok = self.ok_max.replace(depth);
        let saved_bad = self.bad_min.take();

        let body = self.grammar.rules()[index].body;
        let end = self.expr(&body, pos, depth + 1)?;

        let sub_fail = self.furthest.take();
        let sub_ok = self.ok_max.unwrap_or(depth);
        let sub_bad = self.bad_min;

        self.furthest = saved_fail;
        apply(&mut self.furthest, sub_fail);
        self.ok_max = merge_max(saved_ok, Some(sub_ok));
        self.bad_min = merge_min(saved_bad, sub_bad);

        self.store(Entry {
            rule: index,
            pos,
            end,
            fail: sub_fail,
            ok: sub_ok - depth,
            bad: sub_bad.map(|b| b - depth),
        })?;
        Ok(end)
    }

    /// Whether `e` matches at `pos`, with no tree built. Same as the interpreter.
    fn expr(&mut self, e: &Expr, pos: usize, depth: usize) -> Result<Option<usize>, MemoFull> {
        Ok(match *e {
            Expr::Lit(bytes) => {
                if self
                    .input
                    .get(pos..)
                    .is_some_and(|rest| rest.starts_with(bytes))
                {
                    Some(pos + bytes.len())
                } else {
                    self.fail(pos, Expected::Literal(bytes));
                    None
                }
            }
            Expr::Range(lo, hi) => match self.input.get(pos) {
                Some(&b) if (lo..=hi).contains(&b) => Some(pos + 1),
                _ => {
                    self.fail(pos, Expected::Range(lo, hi));
                    None
                }
            },
            Expr::Seq(items) => {
                let mut p = pos;
                let mut ok = true;
                for item in items {
                    match self.expr(item, p, depth)? {
                        Some(np) => p = np,
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok { Some(p) } else { None }
            }
            Expr::Choice(alts) => {
                let mut found = None;
                for alt in alts {
                    if let Some(np) = self.expr(alt, pos, depth)? {
                        found = Some(np);
                        break;
                    }
                }
                found
            }
            Expr::Repeat(inner) => {
                let mut p = pos;
                while let Some(np) = self.expr(inner, p, depth)? {
                    if np > p {
                        p = np;
                    } else {
                        break;
                    }
                }
                Some(p)
            }
            Expr::Optional(inner) => Some(self.expr(inner, pos, depth)?.unwrap_or(pos)),
            Expr::Call(index) => self.call(index, pos, depth)?,
        })
    }

    /// Builds the tree for `e`, which is known to match at `pos`. Every
    /// decision uses `expr`, so no subtree is built and then thrown away.
    fn build(
        &mut self,
        e: &Expr,
        pos: usize,
        depth: usize,
        kids: &mut Vec<Tree>,
    ) -> Result<Option<usize>, MemoFull> {
        Ok(match *e {
            Expr::Lit(bytes) => Some(pos + bytes.len()),
            Expr::Range(_, _) => Some(pos + 1),
            Expr::Seq(items) => {
                let mut p = pos;
                for item in items {
                    match self.build(item, p, depth, kids)? {
                        Some(np) => p = np,
                        None => return Ok(None),
                    }
                }
                Some(p)
            }
            Expr::Choice(alts) => {
                let mut found = None;
                for alt in alts {
                    if self.expr(alt, pos, depth)?.is_some() {
                        found = self.build(alt, pos, depth, kids)?;
                        break;
                    }
                }
                found
            }
            Expr::Repeat(inner) => {
                let mut p = pos;
                while let Some(np) = self.expr(inner, p, depth)? {
                    self.build(inner, p, depth, kids)?;
                    if np > p {
                        p = np;
                    } else {
                        break;
                    }
                }
                Some(p)
            }
            Expr::Optional(inner) => {
                if self.expr(inner, pos, depth)?.is_some() {
                    self.build(inner, pos, depth, kids)?
                } else {
                    Some(pos)
                }
            }
            Expr::Call(index) => {
                let mut sub = Vec::new();
                let body = self.grammar.rules()[index].body;
                match self.build(&body, pos, depth + 1, &mut sub)? {
                    Some(end) => {
                        kids.push(Tree {
                            rule: index,
                            start: pos,
                            end,
                            children: sub,
                        });
                        Some(end)
                    }
                    None => None,
                }
            }
        })
    }
}

impl Grammar {
    /// Parses all of `input` with rule `rule`, as [`Grammar::parse`] does, but
    /// with a memoised packrat matcher that uses `memo` for its storage.
    ///
    /// The result is the same as [`Grammar::parse`]'s, unless the memo table
    /// fills up, in which case this returns [`PackratError::MemoFull`].
    /// Allocates only for the tree.
    ///
    /// ```
    /// use tpt_dev_dsl::{Expr, Grammar, Memo, Rule};
    ///
    /// const DIGIT: Expr = Expr::Range(b'0', b'9');
    /// const NUMBER: Grammar = Grammar::new(&[Rule {
    ///     name: "number",
    ///     body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
    /// }]);
    /// let mut memo = Memo::<64>::new();
    /// let tree = NUMBER.parse_packrat(0, b"2024", &mut memo).unwrap();
    /// assert_eq!((tree.start, tree.end), (0, 4));
    /// ```
    pub fn parse_packrat<const N: usize>(
        &self,
        rule: usize,
        input: &[u8],
        memo: &mut Memo<N>,
    ) -> Result<Tree, PackratError> {
        let mut m = self.matcher(input, memo);
        let end = m.call(rule, 0, 0).map_err(|_| PackratError::MemoFull)?;
        let (furthest, expected) = m.furthest.unwrap_or((0, Expected::EndOfInput));
        match end {
            Some(end) if end == input.len() => {
                let mut kids = Vec::new();
                m.build(&Expr::Call(rule), 0, 0, &mut kids)
                    .map_err(|_| PackratError::MemoFull)?;
                Ok(kids.pop().unwrap_or(Tree {
                    rule,
                    start: 0,
                    end,
                    children: Vec::new(),
                }))
            }
            Some(end) if furthest > end => Err(PackratError::Parse(ParseError {
                position: furthest,
                expected,
            })),
            Some(end) => Err(PackratError::Parse(ParseError {
                position: end,
                expected: Expected::EndOfInput,
            })),
            None => Err(PackratError::Parse(ParseError {
                position: furthest,
                expected,
            })),
        }
    }

    /// Returns `true` if rule `rule` matches all of `input`, as
    /// [`Grammar::recognise`] does, with the packrat matcher.
    pub fn recognise_packrat<const N: usize>(
        &self,
        rule: usize,
        input: &[u8],
        memo: &mut Memo<N>,
    ) -> Result<bool, MemoFull> {
        let mut m = self.matcher(input, memo);
        Ok(m.call(rule, 0, 0)? == Some(input.len()))
    }

    fn matcher<'g, 'i, 'm, const N: usize>(
        &'g self,
        input: &'i [u8],
        memo: &'m mut Memo<N>,
    ) -> Matcher<'g, 'i, 'm, N> {
        Matcher {
            grammar: self,
            input,
            memo,
            furthest: None,
            ok_max: None,
            bad_min: None,
        }
    }
}

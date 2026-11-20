//! A resumable packrat matcher, used by [`PackratStream`](crate::PackratStream).
//!
//! It makes the same decisions as the recursive matcher in `packrat.rs`, with
//! the same memo table, depth rules and furthest-failure report. The
//! difference is where its state lives. Calls, choices and repeats sit on
//! explicit stacks, not on the call stack, so the matcher can stop when it
//! needs a byte that has not arrived yet, and carry on from the same point
//! when the next chunk comes.
//!
//! The matcher reads input only at positions it still has to decide. Once no
//! decision can read a position again, the bytes there are dropped, and so are
//! the memo entries for it. [`Machine::floor`] gives that lowest position.
//! Tree nodes are built as matching goes, so the input is never read again to
//! build them.

use alloc::vec::Vec;

use crate::grammar::{Expr, Grammar, Rule};
use crate::matcher::{Expected, MAX_DEPTH, Tree};
use crate::packrat::{Fail, MemoFull, apply, hash, merge_max, merge_min};

/// The bytes the matcher can see: positions below `end` that are still kept.
pub(crate) struct Input<'a> {
    /// Ring buffer. The byte at position `p` is at `ring[p % ring.len()]`.
    pub(crate) ring: &'a [u8],
    /// One past the last byte that has arrived.
    pub(crate) end: usize,
    /// Whether the input has ended. Until it has, a match that needs a byte
    /// past `end` waits.
    pub(crate) eof: bool,
}

impl Input<'_> {
    fn get(&self, pos: usize) -> Option<u8> {
        (pos < self.end).then(|| self.ring[pos % self.ring.len()])
    }
}

/// What the matcher does next.
#[derive(Clone, Copy)]
enum Mode {
    /// Match an expression at a position, at a call depth.
    Eval(&'static Expr, usize, usize),
    /// Match a rule at a position, at a call depth.
    Call(usize, usize, usize),
    /// Hand a result to the frame on top of the stack. `None` is a failure.
    Ret(Option<usize>),
    /// The start rule has returned, and its result is in `root`.
    Done,
}

/// An expression that is waiting for the result of a sub-expression.
#[derive(Clone, Copy)]
enum Frame {
    /// A sequence never reads at a position it has already passed, so it
    /// does not record one. The item in progress holds the position.
    Seq {
        items: &'static [Expr],
        /// Index of the next item to match.
        next: usize,
        depth: usize,
        /// Length of `kids` when the sequence started.
        mark: usize,
    },
    Choice {
        alts: &'static [Expr],
        /// Index of the next alternative to try.
        next: usize,
        /// Position every alternative starts at.
        pos: usize,
        depth: usize,
        mark: usize,
    },
    Repeat {
        inner: &'static Expr,
        /// Position after the last repetition that made progress.
        pos: usize,
        depth: usize,
    },
    Optional {
        pos: usize,
    },
    /// A rule call, with the failure and depth state of its caller.
    Call {
        rule: usize,
        pos: usize,
        depth: usize,
        mark: usize,
        saved_fail: Fail,
        saved_ok: Option<usize>,
        saved_bad: Option<usize>,
    },
}

/// A memoised result, as in `packrat.rs`, with the tree the call built.
#[derive(Clone)]
struct Entry {
    rule: usize,
    pos: usize,
    /// The end of the match, or `None` if the rule failed.
    end: Option<usize>,
    fail: Fail,
    ok: usize,
    bad: Option<usize>,
    /// The subtree, present exactly when `end` is.
    tree: Option<Tree>,
}

impl Entry {
    fn valid_at(&self, depth: usize) -> bool {
        depth + self.ok <= MAX_DEPTH && self.bad.is_none_or(|b| depth + b > MAX_DEPTH)
    }
}

enum Slot {
    Found(usize),
    Vacant(usize),
}

fn probe(memo: &[Option<Entry>], rule: usize, pos: usize) -> Result<Slot, MemoFull> {
    let n = memo.len();
    if n == 0 {
        return Err(MemoFull);
    }
    let mut i = hash(rule, pos) % n;
    for _ in 0..n {
        match &memo[i] {
            None => return Ok(Slot::Vacant(i)),
            Some(e) if e.rule == rule && e.pos == pos => return Ok(Slot::Found(i)),
            Some(_) => i = if i + 1 == n { 0 } else { i + 1 },
        }
    }
    Err(MemoFull)
}

fn min_opt(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, None) | (None, x) => x,
    }
}

/// The resumable matcher. Its state persists between calls to [`Machine::run`].
pub(crate) struct Machine {
    grammar: Grammar,
    /// The start rule, returned by [`Machine::into_tree`] if it matches.
    rule: usize,
    mode: Mode,
    stack: Vec<Frame>,
    /// Subtrees built so far, for the frames still open.
    kids: Vec<Tree>,
    /// Failures from the current call, as the one-shot matcher keeps them.
    furthest: Fail,
    ok_max: Option<usize>,
    bad_min: Option<usize>,
    memo: Vec<Option<Entry>>,
    /// The start rule's result, once it has returned: its end, or `None`.
    root: Option<Option<usize>>,
}

impl Machine {
    /// A matcher for rule `rule`, with `memo_slots` memo slots.
    pub(crate) fn new(grammar: Grammar, rule: usize, memo_slots: usize) -> Self {
        Self {
            grammar,
            rule,
            mode: Mode::Call(rule, 0, 0),
            stack: Vec::new(),
            kids: Vec::new(),
            furthest: None,
            ok_max: None,
            bad_min: None,
            memo: (0..memo_slots).map(|_| None).collect(),
            root: None,
        }
    }

    /// Runs until the start rule returns, or until it needs a byte that has
    /// not arrived. Returns `true` once it has returned.
    pub(crate) fn run(&mut self, input: &Input<'_>) -> Result<bool, MemoFull> {
        loop {
            match self.mode {
                Mode::Done => return Ok(true),
                Mode::Eval(e, pos, depth) => {
                    if !self.eval(e, pos, depth, input) {
                        return Ok(false);
                    }
                }
                Mode::Call(rule, pos, depth) => self.call(rule, pos, depth)?,
                Mode::Ret(result) => self.ret(result)?,
            }
        }
    }

    /// The lowest position that matching can still read, or `None` if it
    /// will read none. Everything below it can be dropped.
    pub(crate) fn floor(&self) -> Option<usize> {
        let mut floor = match self.mode {
            Mode::Eval(_, pos, _) | Mode::Call(_, pos, _) | Mode::Ret(Some(pos)) => Some(pos),
            Mode::Ret(None) | Mode::Done => None,
        };
        for frame in &self.stack {
            let pos = match *frame {
                Frame::Choice { pos, .. }
                | Frame::Repeat { pos, .. }
                | Frame::Optional { pos, .. } => Some(pos),
                Frame::Seq { .. } | Frame::Call { .. } => None,
            };
            floor = min_opt(floor, pos);
        }
        floor
    }

    /// The start rule's result: `Some(end)` if it matched, `Some(None)` if it
    /// failed, and `None` while it has not returned.
    pub(crate) fn root(&self) -> Option<Option<usize>> {
        self.root
    }

    /// The furthest failure, as the one-shot matcher reports it.
    pub(crate) fn furthest(&self) -> Option<(usize, Expected)> {
        self.furthest
    }

    /// The start rule's tree, for a match ending at `end`.
    pub(crate) fn into_tree(mut self, end: usize) -> Tree {
        self.kids.pop().unwrap_or(Tree {
            rule: self.rule,
            start: 0,
            end,
            children: Vec::new(),
        })
    }

    fn fail(&mut self, pos: usize, what: Expected) {
        apply(&mut self.furthest, Some((pos, what)));
    }

    /// Matches expression `e`. Returns `false` if it must wait for input.
    fn eval(&mut self, e: &'static Expr, pos: usize, depth: usize, input: &Input<'_>) -> bool {
        self.mode = match *e {
            Expr::Lit(bytes) => {
                let end = pos + bytes.len();
                if end > input.end && !input.eof {
                    return false;
                }
                if end <= input.end
                    && (0..bytes.len()).all(|k| input.get(pos + k) == Some(bytes[k]))
                {
                    Mode::Ret(Some(end))
                } else {
                    self.fail(pos, Expected::Literal(bytes));
                    Mode::Ret(None)
                }
            }
            Expr::Range(lo, hi) => {
                if pos >= input.end && !input.eof {
                    return false;
                }
                match input.get(pos) {
                    Some(b) if (lo..=hi).contains(&b) => Mode::Ret(Some(pos + 1)),
                    _ => {
                        self.fail(pos, Expected::Range(lo, hi));
                        Mode::Ret(None)
                    }
                }
            }
            Expr::Seq(items) => match items.first() {
                None => Mode::Ret(Some(pos)),
                Some(first) => {
                    self.stack.push(Frame::Seq {
                        items,
                        next: 1,
                        depth,
                        mark: self.kids.len(),
                    });
                    Mode::Eval(first, pos, depth)
                }
            },
            Expr::Choice(alts) => match alts.first() {
                None => Mode::Ret(None),
                Some(first) => {
                    self.stack.push(Frame::Choice {
                        alts,
                        next: 1,
                        pos,
                        depth,
                        mark: self.kids.len(),
                    });
                    Mode::Eval(first, pos, depth)
                }
            },
            Expr::Repeat(inner) => {
                self.stack.push(Frame::Repeat { inner, pos, depth });
                Mode::Eval(inner, pos, depth)
            }
            Expr::Optional(inner) => {
                self.stack.push(Frame::Optional { pos });
                Mode::Eval(inner, pos, depth)
            }
            Expr::Call(index) => Mode::Call(index, pos, depth),
        };
        true
    }

    /// Starts a rule call, or answers it from the memo table.
    fn call(&mut self, rule: usize, pos: usize, depth: usize) -> Result<(), MemoFull> {
        if depth > MAX_DEPTH {
            self.bad_min = merge_min(self.bad_min, Some(depth));
            self.fail(pos, Expected::Recursion);
            self.mode = Mode::Ret(None);
            return Ok(());
        }
        if let Slot::Found(i) = self.locate(rule, pos)?
            && let Some(e) = self.memo[i].as_ref().filter(|e| e.valid_at(depth))
        {
            let (fail, ok, bad, end, tree) = (e.fail, e.ok, e.bad, e.end, e.tree.clone());
            apply(&mut self.furthest, fail);
            self.ok_max = merge_max(self.ok_max, Some(depth + ok));
            self.bad_min = merge_min(self.bad_min, bad.map(|b| depth + b));
            if let Some(tree) = tree {
                self.kids.push(tree);
            }
            self.mode = Mode::Ret(end);
            return Ok(());
        }

        // Measure this call on its own. `ret` folds the result back in.
        let rules: &'static [Rule] = self.grammar.rules();
        let saved_fail = self.furthest.take();
        let saved_ok = self.ok_max.replace(depth);
        let saved_bad = self.bad_min.take();
        self.stack.push(Frame::Call {
            rule,
            pos,
            depth,
            mark: self.kids.len(),
            saved_fail,
            saved_ok,
            saved_bad,
        });
        self.mode = Mode::Eval(&rules[rule].body, pos, depth + 1);
        Ok(())
    }

    /// Passes `result` to the frame on top of the stack.
    fn ret(&mut self, result: Option<usize>) -> Result<(), MemoFull> {
        let Some(frame) = self.stack.pop() else {
            self.root = Some(result);
            self.mode = Mode::Done;
            return Ok(());
        };
        self.mode = match frame {
            Frame::Seq {
                items,
                next,
                depth,
                mark,
            } => match result {
                Some(np) if next < items.len() => {
                    self.stack.push(Frame::Seq {
                        items,
                        next: next + 1,
                        depth,
                        mark,
                    });
                    Mode::Eval(&items[next], np, depth)
                }
                Some(np) => Mode::Ret(Some(np)),
                None => {
                    self.kids.truncate(mark);
                    Mode::Ret(None)
                }
            },
            Frame::Choice {
                alts,
                next,
                pos,
                depth,
                mark,
            } => match result {
                Some(np) => Mode::Ret(Some(np)),
                None if next < alts.len() => {
                    self.stack.push(Frame::Choice {
                        alts,
                        next: next + 1,
                        pos,
                        depth,
                        mark,
                    });
                    Mode::Eval(&alts[next], pos, depth)
                }
                None => {
                    self.kids.truncate(mark);
                    Mode::Ret(None)
                }
            },
            // A repetition that makes no progress ends the loop, and its
            // subtrees stay, as in the one-shot matcher.
            Frame::Repeat { inner, pos, depth } => match result {
                Some(np) if np > pos => {
                    self.stack.push(Frame::Repeat {
                        inner,
                        pos: np,
                        depth,
                    });
                    Mode::Eval(inner, np, depth)
                }
                _ => Mode::Ret(Some(pos)),
            },
            Frame::Optional { pos, .. } => Mode::Ret(result.or(Some(pos))),
            Frame::Call {
                rule,
                pos,
                depth,
                mark,
                saved_fail,
                saved_ok,
                saved_bad,
            } => {
                let sub_fail = self.furthest.take();
                let sub_ok = self.ok_max.unwrap_or(depth);
                let sub_bad = self.bad_min;
                self.furthest = saved_fail;
                apply(&mut self.furthest, sub_fail);
                self.ok_max = merge_max(saved_ok, Some(sub_ok));
                self.bad_min = merge_min(saved_bad, sub_bad);

                let tree = match result {
                    Some(end) => Some(Tree {
                        rule,
                        start: pos,
                        end,
                        children: self.kids.split_off(mark),
                    }),
                    None => {
                        self.kids.truncate(mark);
                        None
                    }
                };
                self.store(Entry {
                    rule,
                    pos,
                    end: result,
                    fail: sub_fail,
                    ok: sub_ok - depth,
                    bad: sub_bad.map(|b| b - depth),
                    tree: tree.clone(),
                })?;
                if let Some(tree) = tree {
                    self.kids.push(tree);
                }
                Mode::Ret(result)
            }
        };
        Ok(())
    }

    /// Finds the slot for `(rule, pos)`. If the table is full, first drops the
    /// entries that can no longer be looked up, then tries again.
    fn locate(&mut self, rule: usize, pos: usize) -> Result<Slot, MemoFull> {
        match probe(&self.memo, rule, pos) {
            Err(MemoFull) => {
                let floor = self.floor();
                self.sweep(floor);
                probe(&self.memo, rule, pos)
            }
            found => found,
        }
    }

    fn store(&mut self, entry: Entry) -> Result<(), MemoFull> {
        let i = match self.locate(entry.rule, entry.pos)? {
            Slot::Found(i) | Slot::Vacant(i) => i,
        };
        self.memo[i] = Some(entry);
        Ok(())
    }

    /// Drops every entry below `floor`. No later call starts below the floor,
    /// so those entries can never be looked up again.
    fn sweep(&mut self, floor: Option<usize>) {
        let live: Vec<Entry> = self
            .memo
            .iter_mut()
            .filter_map(Option::take)
            .filter(|e| floor.is_some_and(|f| e.pos >= f))
            .collect();
        for entry in live {
            if let Ok(Slot::Found(i) | Slot::Vacant(i)) = probe(&self.memo, entry.rule, entry.pos) {
                self.memo[i] = Some(entry);
            }
        }
    }
}

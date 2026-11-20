//! Streaming front end for the packrat matcher. Byte chunks go through
//! `tpt-dev-ast`'s [`Feed`] driver, which calls [`GrammarLexer`]. Each byte of
//! the grammar's alphabet becomes one terminal. The bytes go into a ring
//! buffer, and the resumable matcher in `resumable.rs` runs over them as they
//! arrive.
//!
//! The matcher keeps only the bytes it can still read. A `Choice` can backtrack
//! to where it started, so the ring must hold the input from the lowest
//! position still under decision to the newest byte. Its size is fixed by the
//! caller. If that span is longer, the stream stops with
//! [`StreamError::TokensFull`] and never buffers without limit.

use tpt_dev_ast::{Fault, Feed, Lexer, ParseError as AstError, Step};

use crate::grammar::{Expr, Grammar};
use crate::matcher::{Expected, ParseError, Tree};
use crate::packrat::MemoFull;
use crate::resumable::{Input, Machine};

/// Classifies bytes as terminals of a grammar's alphabet. The alphabet is every
/// byte named by a `Lit` or a `Range` in the grammar.
///
/// A terminal is the byte itself, so it is 1 byte long. A byte outside the
/// alphabet is an invalid token, and the lexer reports it at once. That
/// includes the lead byte of a multi-byte UTF-8 character that the grammar
/// does not name. A grammar that names multi-byte characters has their bytes
/// in its alphabet, so they arrive as ordinary terminals, split across chunks
/// or not.
///
/// ```
/// use tpt_dev_dsl::{GrammarLexer, Expr, Grammar, Rule};
/// use tpt_dev_ast::{Lexer, Step};
///
/// const PLUS: Grammar = Grammar::new(&[Rule { name: "plus", body: Expr::Lit(b"+") }]);
/// let lexer = GrammarLexer::new(&PLUS);
/// assert_eq!(lexer.lex(b"+", false), Step::Token { token: b'+', len: 1 });
/// assert_eq!(lexer.lex(b"x", false), Step::Invalid { at: 0 });
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrammarLexer {
    alphabet: [u64; 4],
}

impl GrammarLexer {
    /// The lexer for `grammar`'s alphabet.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ast::{Lexer, Step};
    /// use tpt_dev_dsl::{Expr, Grammar, GrammarLexer, Rule};
    /// const PLUS: Grammar = Grammar::new(&[Rule { name: "plus", body: Expr::Lit(b"+") }]);
    /// let lexer = GrammarLexer::new(&PLUS);
    /// assert_eq!(lexer.lex(b"+", false), Step::Token { token: b'+', len: 1 });
    /// assert_eq!(lexer.lex(b"x", false), Step::Invalid { at: 0 });
    /// ```
    pub fn new(grammar: &Grammar) -> Self {
        let mut alphabet = [0; 4];
        for rule in grammar.rules() {
            mark(&rule.body, &mut alphabet);
        }
        Self { alphabet }
    }

    fn contains(&self, byte: u8) -> bool {
        self.alphabet[usize::from(byte >> 6)] & (1 << (byte & 63)) != 0
    }
}

fn add_byte(set: &mut [u64; 4], b: u8) {
    set[usize::from(b >> 6)] |= 1 << (b & 63);
}

fn mark(e: &Expr, set: &mut [u64; 4]) {
    match *e {
        Expr::Lit(bytes) => {
            for &b in bytes {
                add_byte(set, b);
            }
        }
        Expr::Range(lo, hi) => {
            for b in lo..=hi {
                add_byte(set, b);
            }
        }
        Expr::Seq(items) | Expr::Choice(items) => {
            for item in items {
                mark(item, set);
            }
        }
        Expr::Repeat(inner) | Expr::Optional(inner) => mark(inner, set),
        Expr::Call(_) => {}
    }
}

impl Lexer for GrammarLexer {
    /// A terminal: the byte itself.
    type Token<'a> = u8;

    fn lex(&self, input: &[u8], _eof: bool) -> Step<u8> {
        match input.first() {
            None => Step::Incomplete,
            Some(&b) if self.contains(b) => Step::Token { token: b, len: 1 },
            Some(_) => Step::Invalid { at: 0 },
        }
    }
}

/// Why a streamed parse failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamError {
    /// A byte outside the grammar's alphabet, at this stream offset.
    Invalid {
        /// Offset of the byte, counted over the whole stream.
        offset: usize,
    },
    /// The matcher needs more than `TOKENS` bytes at once. The bytes from the
    /// lowest position still under decision to the newest one do not fit. The
    /// stream stopped at `offset`, and the bytes from there on were not read.
    TokensFull {
        /// Offset of the first byte that did not fit.
        offset: usize,
    },
    /// The terminals do not match the grammar. This is the matcher's own error,
    /// with positions counted in terminals (bytes).
    Parse(ParseError),
    /// The memo table filled up before matching finished, even after the
    /// entries that could no longer be used were dropped.
    MemoFull,
}

/// Parses a stream of byte chunks against one rule of a grammar.
///
/// `TOKENS` is the ring buffer's capacity in bytes. The stream keeps only the
/// bytes the matcher can still read, so `TOKENS` bounds how far a decision may
/// reach back, not how long the input is. `MEMO` is the number of memo slots.
/// Memo entries below the lowest live position are dropped when the table
/// fills, so `MEMO` bounds the live entries, not the whole input. The matcher's
/// state lives on the heap.
///
/// Feed chunks with [`feed`](Self::feed), then call [`finish`](Self::finish)
/// or [`finish_recognise`](Self::finish_recognise). The results match
/// [`Grammar::parse_packrat`] and [`Grammar::recognise_packrat`] on the whole
/// input, except that the stream can succeed where the one-shot matcher with
/// the same memo size runs out of memo slots. Once a call returns an error,
/// every later call returns it too.
///
/// ```
/// use tpt_dev_dsl::{Expr, Grammar, PackratStream, Rule};
///
/// const DIGIT: Expr = Expr::Range(b'0', b'9');
/// const NUMBER: Grammar = Grammar::new(&[Rule {
///     name: "number",
///     body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
/// }]);
///
/// let mut stream: PackratStream<64, 16> = PackratStream::new(NUMBER, 0);
/// stream.feed(b"20").unwrap();
/// stream.feed(b"24").unwrap();
/// let tree = stream.finish().unwrap();
/// assert_eq!((tree.start, tree.end), (0, 4));
/// ```
pub struct PackratStream<const TOKENS: usize, const MEMO: usize> {
    // A terminal is never split across chunks, so the driver never holds a
    // partial token and its carry buffer stays unused. It needs one byte.
    feed: Feed<GrammarLexer, 1>,
    /// Ring buffer holding the bytes at positions `base..end`.
    window: [u8; TOKENS],
    /// Lowest position still kept. Bytes below it have been dropped.
    base: usize,
    /// One past the last byte read.
    end: usize,
    /// Set by `finish`: no more bytes will arrive.
    eof: bool,
    machine: Machine,
    failed: Option<StreamError>,
}

impl<const TOKENS: usize, const MEMO: usize> PackratStream<TOKENS, MEMO> {
    /// A stream that will match rule `rule` of `grammar`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dsl::{Expr, Grammar, Rule};
    /// const DIGIT: Expr = Expr::Range(b'0', b'9');
    /// const NUMBER: Grammar = Grammar::new(&[Rule {
    ///     name: "number",
    ///     body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
    /// }]);
    /// use tpt_dev_dsl::PackratStream;
    /// let mut stream: PackratStream<16, 16> = PackratStream::new(NUMBER, 0);
    /// stream.feed(b"20").unwrap();
    /// stream.feed(b"24").unwrap();
    /// let tree = stream.finish().unwrap();
    /// assert_eq!((tree.start, tree.end), (0, 4));
    /// ```
    pub fn new(grammar: Grammar, rule: usize) -> Self {
        Self {
            feed: Feed::new(GrammarLexer::new(&grammar)),
            window: [0; TOKENS],
            base: 0,
            end: 0,
            eof: false,
            machine: Machine::new(grammar, rule, MEMO),
            failed: None,
        }
    }

    /// Total bytes read so far.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dsl::{Expr, Grammar, Rule};
    /// const DIGIT: Expr = Expr::Range(b'0', b'9');
    /// const NUMBER: Grammar = Grammar::new(&[Rule {
    ///     name: "number",
    ///     body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
    /// }]);
    /// use tpt_dev_dsl::PackratStream;
    /// let mut stream: PackratStream<16, 16> = PackratStream::new(NUMBER, 0);
    /// assert_eq!(stream.consumed(), 0);
    /// stream.feed(b"20").unwrap();
    /// assert_eq!(stream.consumed(), 2);
    /// ```
    pub fn consumed(&self) -> usize {
        self.feed.consumed()
    }

    /// Feeds the next chunk. Chunks may split the input anywhere.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dsl::{Expr, Grammar, Rule};
    /// const DIGIT: Expr = Expr::Range(b'0', b'9');
    /// const NUMBER: Grammar = Grammar::new(&[Rule {
    ///     name: "number",
    ///     body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
    /// }]);
    /// use tpt_dev_dsl::PackratStream;
    /// let mut stream: PackratStream<16, 16> = PackratStream::new(NUMBER, 0);
    /// // Chunks may split the input anywhere, here inside the number.
    /// stream.feed(b"2").unwrap();
    /// stream.feed(b"0").unwrap();
    /// stream.feed(b"24").unwrap();
    /// assert!(stream.finish_recognise().unwrap());
    /// ```
    pub fn feed(&mut self, chunk: &[u8]) -> Result<(), StreamError> {
        self.check()?;
        let result = self.push(chunk);
        self.record(result)
    }

    /// Declares the end of input, then returns the tree of rule `rule` over
    /// all of it. Same result as [`Grammar::parse_packrat`] on the whole input.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dsl::{Expr, Grammar, Rule};
    /// const DIGIT: Expr = Expr::Range(b'0', b'9');
    /// const NUMBER: Grammar = Grammar::new(&[Rule {
    ///     name: "number",
    ///     body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
    /// }]);
    /// use tpt_dev_dsl::PackratStream;
    /// let mut stream: PackratStream<16, 16> = PackratStream::new(NUMBER, 0);
    /// stream.feed(b"20").unwrap();
    /// stream.feed(b"24").unwrap();
    /// let tree = stream.finish().unwrap();
    /// assert_eq!(tree.end, 4);
    /// ```
    pub fn finish(mut self) -> Result<Tree, StreamError> {
        self.check()?;
        let result = self.close();
        self.record(result)?;
        let total = self.end;
        let root = self.machine.root().unwrap_or(None);
        let (furthest, expected) = self.machine.furthest().unwrap_or((0, Expected::EndOfInput));
        match root {
            Some(end) if end == total => Ok(self.machine.into_tree(total)),
            Some(end) if furthest > end => Err(StreamError::Parse(ParseError {
                position: furthest,
                expected,
            })),
            Some(end) => Err(StreamError::Parse(ParseError {
                position: end,
                expected: Expected::EndOfInput,
            })),
            None => Err(StreamError::Parse(ParseError {
                position: furthest,
                expected,
            })),
        }
    }

    /// Declares the end of input, then checks that rule `rule` matches all of
    /// it. Same result as [`Grammar::recognise_packrat`] on the whole input.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dsl::{Expr, Grammar, Rule};
    /// const DIGIT: Expr = Expr::Range(b'0', b'9');
    /// const NUMBER: Grammar = Grammar::new(&[Rule {
    ///     name: "number",
    ///     body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
    /// }]);
    /// use tpt_dev_dsl::PackratStream;
    /// let mut stream: PackratStream<16, 16> = PackratStream::new(NUMBER, 0);
    /// stream.feed(b"20").unwrap();
    /// stream.feed(b"24").unwrap();
    /// assert!(stream.finish_recognise().unwrap());
    /// ```
    pub fn finish_recognise(mut self) -> Result<bool, StreamError> {
        self.check()?;
        let result = self.close();
        self.record(result)?;
        let total = self.end;
        Ok(self.machine.root() == Some(Some(total)))
    }

    /// Feeds the bytes one at a time, and runs the matcher when the chunk ends.
    fn push(&mut self, chunk: &[u8]) -> Result<(), StreamError> {
        for &byte in chunk {
            if self.end - self.base == TOKENS {
                self.advance()?;
                self.compact();
                if self.end - self.base == TOKENS {
                    return Err(StreamError::TokensFull { offset: self.end });
                }
            }
            let at = self.end;
            let mut full = false;
            let Self {
                feed, window, base, ..
            } = self;
            let mut sink = |t: u8| {
                if at - *base == TOKENS {
                    full = true;
                    return Err(Fault::Syntax);
                }
                window[at % TOKENS] = t;
                Ok(())
            };
            if let Err(e) = feed.feed(&[byte], &mut sink) {
                return Err(lex_error(e, full));
            }
            self.end += 1;
        }
        self.advance()
    }

    /// Declares the end of input, and runs the matcher over what is left.
    fn close(&mut self) -> Result<(), StreamError> {
        self.advance()?;
        self.compact();
        self.eof = true;
        let at = self.end;
        let base = self.base;
        let mut full = false;
        let result = {
            let Self {
                feed, window, end, ..
            } = self;
            let mut sink = |t: u8| {
                if at - base == TOKENS {
                    full = true;
                    return Err(Fault::Syntax);
                }
                window[at % TOKENS] = t;
                *end += 1;
                Ok(())
            };
            feed.finish(&mut sink)
        };
        if let Err(e) = result {
            return Err(lex_error(e, full));
        }
        self.advance()
    }

    /// Runs the matcher as far as the bytes read so far allow.
    fn advance(&mut self) -> Result<(), StreamError> {
        let input = Input {
            ring: &self.window,
            end: self.end,
            eof: self.eof,
        };
        self.machine
            .run(&input)
            .map(|_| ())
            .map_err(|MemoFull| StreamError::MemoFull)
    }

    /// Drops the bytes the matcher can no longer read.
    fn compact(&mut self) {
        let floor = self.machine.floor().unwrap_or(self.end).min(self.end);
        self.base = self.base.max(floor);
    }

    fn check(&self) -> Result<(), StreamError> {
        match &self.failed {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    /// Makes an error sticky, so every later call returns it.
    fn record<T>(&mut self, result: Result<T, StreamError>) -> Result<T, StreamError> {
        if let Err(e) = &result {
            self.failed = Some(e.clone());
        }
        result
    }
}

/// Turns a driver error into a stream error. `full` says the sink refused a
/// terminal because the ring is full. Any other error comes from the lexer.
fn lex_error(e: AstError, full: bool) -> StreamError {
    let offset = match e {
        AstError::Lex { offset }
        | AstError::Syntax { offset }
        | AstError::TrailingData { offset }
        | AstError::DepthExceeded { offset, .. }
        | AstError::TokenTooLong { offset, .. }
        | AstError::UnexpectedEnd { offset } => offset,
    };
    if full {
        StreamError::TokensFull { offset }
    } else {
        StreamError::Invalid { offset }
    }
}

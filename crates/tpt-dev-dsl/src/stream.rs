//! Streaming front end for the packrat matcher. Byte chunks go through
//! `tpt-dev-ast`'s [`Feed`] driver, which calls [`GrammarLexer`]. Each byte of
//! the grammar's alphabet becomes one terminal. Terminals are kept in a
//! fixed-capacity buffer, and the packrat matcher runs over that buffer when
//! the input ends.
//!
//! Packrat matching needs random access to earlier terminals, because a
//! `Choice` can backtrack over any amount of input. So the matcher cannot run
//! before the end of the input, and the terminal buffer holds the whole
//! accepted input. Its capacity is fixed by the caller. Input that needs more
//! terminals than that is rejected with [`StreamError::TokensFull`], never
//! buffered without limit.

use tpt_dev_ast::{Fault, Feed, Lexer, ParseError as AstError, Step};

use crate::grammar::{Expr, Grammar};
use crate::matcher::{ParseError, Tree};
use crate::packrat::{Memo, PackratError};

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
    /// A terminal: the byte, which is also its position in the terminal buffer.
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
    /// The terminal buffer is full. The stream stopped at `offset`, and the
    /// bytes after it were not read.
    TokensFull {
        /// Offset of the first byte that did not fit.
        offset: usize,
    },
    /// The terminals do not match the grammar. This is the matcher's own error,
    /// with positions counted in terminals (bytes).
    Parse(ParseError),
    /// The memo table filled up before matching finished.
    MemoFull,
}

/// Parses a stream of byte chunks against one rule of a grammar.
///
/// `TOKENS` is the terminal buffer's capacity, in bytes, and it is stored in
/// the struct, so a stack-allocated stream uses a fixed amount of memory.
///
/// Feed chunks with [`feed`](Self::feed), then call [`finish`](Self::finish)
/// or [`finish_recognise`](Self::finish_recognise) with a memo table. The
/// results match [`Grammar::parse_packrat`] and [`Grammar::recognise_packrat`]
/// on the whole input. Once a call returns an error, every later call returns
/// it too.
///
/// ```
/// use tpt_dev_dsl::{Expr, Grammar, Memo, PackratStream, Rule};
///
/// const DIGIT: Expr = Expr::Range(b'0', b'9');
/// const NUMBER: Grammar = Grammar::new(&[Rule {
///     name: "number",
///     body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
/// }]);
///
/// let mut stream: PackratStream<64> = PackratStream::new(NUMBER, 0);
/// stream.feed(b"20").unwrap();
/// stream.feed(b"24").unwrap();
/// let mut memo = Memo::<16>::new();
/// let tree = stream.finish(&mut memo).unwrap();
/// assert_eq!((tree.start, tree.end), (0, 4));
/// ```
pub struct PackratStream<const TOKENS: usize> {
    grammar: Grammar,
    rule: usize,
    // A terminal is never split across chunks, so the driver never holds a
    // partial token and its carry buffer stays unused. It needs one byte.
    feed: Feed<GrammarLexer, 1>,
    tokens: [u8; TOKENS],
    len: usize,
    /// Set by the sink when the terminal buffer is full.
    full: bool,
    failed: Option<StreamError>,
}

/// A sink that stores each terminal, or stops the driver when the buffer is full.
fn store<'a, const T: usize>(
    tokens: &'a mut [u8; T],
    len: &'a mut usize,
    full: &'a mut bool,
) -> impl FnMut(u8) -> Result<(), Fault> + 'a {
    move |b| {
        if *len == T {
            *full = true;
            return Err(Fault::Syntax);
        }
        tokens[*len] = b;
        *len += 1;
        Ok(())
    }
}

impl<const TOKENS: usize> PackratStream<TOKENS> {
    /// A stream that will match rule `rule` of `grammar`.
    pub fn new(grammar: Grammar, rule: usize) -> Self {
        Self {
            feed: Feed::new(GrammarLexer::new(&grammar)),
            grammar,
            rule,
            tokens: [0; TOKENS],
            len: 0,
            full: false,
            failed: None,
        }
    }

    /// Total bytes read so far.
    pub fn consumed(&self) -> usize {
        self.feed.consumed()
    }

    /// Feeds the next chunk. Chunks may split the input anywhere.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<(), StreamError> {
        self.check()?;
        let result = {
            let mut sink = store(&mut self.tokens, &mut self.len, &mut self.full);
            self.feed.feed(chunk, &mut sink)
        };
        self.settle(result)
    }

    /// Declares the end of input, then matches rule `rule` over all of it.
    /// Returns the same tree as [`Grammar::parse_packrat`] on the whole input.
    pub fn finish<const N: usize>(mut self, memo: &mut Memo<N>) -> Result<Tree, StreamError> {
        self.end()?;
        let input = &self.tokens[..self.len];
        self.grammar
            .parse_packrat(self.rule, input, memo)
            .map_err(|e| match e {
                PackratError::Parse(p) => StreamError::Parse(p),
                PackratError::MemoFull => StreamError::MemoFull,
            })
    }

    /// Declares the end of input, then checks that rule `rule` matches all of
    /// it. Same result as [`Grammar::recognise_packrat`] on the whole input.
    pub fn finish_recognise<const N: usize>(
        mut self,
        memo: &mut Memo<N>,
    ) -> Result<bool, StreamError> {
        self.end()?;
        let input = &self.tokens[..self.len];
        self.grammar
            .recognise_packrat(self.rule, input, memo)
            .map_err(|_| StreamError::MemoFull)
    }

    fn end(&mut self) -> Result<(), StreamError> {
        self.check()?;
        let result = {
            let mut sink = store(&mut self.tokens, &mut self.len, &mut self.full);
            self.feed.finish(&mut sink)
        };
        self.settle(result)
    }

    fn check(&self) -> Result<(), StreamError> {
        match &self.failed {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    fn settle(&mut self, result: Result<(), AstError>) -> Result<(), StreamError> {
        let Err(e) = result else {
            return Ok(());
        };
        let offset = match e {
            AstError::Lex { offset }
            | AstError::Syntax { offset }
            | AstError::TrailingData { offset }
            | AstError::DepthExceeded { offset, .. }
            | AstError::TokenTooLong { offset, .. }
            | AstError::UnexpectedEnd { offset } => offset,
        };
        // Only the sink's `Syntax` fault means the buffer is full. Any other
        // error comes from the lexer.
        let err = if self.full {
            StreamError::TokensFull { offset }
        } else {
            StreamError::Invalid { offset }
        };
        self.failed = Some(err.clone());
        Err(err)
    }
}

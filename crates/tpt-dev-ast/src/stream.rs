//! The chunked driver: feeds byte chunks to the lexer and parser.
//!
//! Tokens that lie wholly inside one chunk are lexed in place, without
//! copying. A token split across chunks is held in a fixed carry buffer of
//! `CARRY` bytes until the rest arrives.

use core::cmp::min;

use crate::parser::{Fault, Parser, Visitor};
use crate::token::{Lex, lex};

/// An error found while parsing. Each variant gives the byte offset in the whole stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Bytes that cannot begin a token.
    Lex {
        /// Offset of the bad byte.
        offset: usize,
    },
    /// A token the grammar does not allow here.
    Syntax {
        /// Offset of the token.
        offset: usize,
    },
    /// A token after the top-level value is complete.
    TrailingData {
        /// Offset of the token.
        offset: usize,
    },
    /// Nesting deeper than `DEPTH`.
    DepthExceeded {
        /// Offset of the opening bracket.
        offset: usize,
        /// The configured depth.
        limit: usize,
    },
    /// A token longer than the carry buffer.
    TokenTooLong {
        /// Offset where the token starts.
        offset: usize,
        /// The carry capacity.
        limit: usize,
    },
    /// The input ended before the top-level value was complete.
    UnexpectedEnd {
        /// Total bytes read.
        offset: usize,
    },
}

impl Fault {
    fn at(self, offset: usize) -> ParseError {
        match self {
            Fault::Syntax => ParseError::Syntax { offset },
            Fault::TrailingData => ParseError::TrailingData { offset },
            Fault::DepthExceeded { limit } => ParseError::DepthExceeded { offset, limit },
            Fault::UnexpectedEnd => ParseError::UnexpectedEnd { offset },
        }
    }
}

/// A streaming parser. It accepts input in chunks of any size.
///
/// `DEPTH` is the maximum nesting, and `CARRY` is the longest token that can
/// cross a chunk boundary. Both are fixed at compile time, so the parser uses
/// a fixed amount of memory whatever the input size.
///
/// ```
/// use tpt_dev_ast::{Event, Stream};
///
/// let mut stream: Stream<4, 16> = Stream::new();
/// let mut count = 0;
/// let mut visit = |_: Event<'_>| count += 1;
/// // The number 1234 is split across two chunks.
/// stream.feed(b"[12", &mut visit).unwrap();
/// stream.feed(b"34]", &mut visit).unwrap();
/// stream.finish(&mut visit).unwrap();
/// assert_eq!(count, 3);
/// ```
pub struct Stream<const DEPTH: usize, const CARRY: usize> {
    parser: Parser<DEPTH>,
    carry: [u8; CARRY],
    carry_len: usize,
    consumed: usize,
}

impl<const DEPTH: usize, const CARRY: usize> Stream<DEPTH, CARRY> {
    /// Creates a stream ready for the first byte.
    ///
    /// Fails to compile if `CARRY` is zero.
    pub const fn new() -> Self {
        const { assert!(CARRY > 0, "the carry buffer needs at least one byte") };
        Self {
            parser: Parser::new(),
            carry: [0; CARRY],
            carry_len: 0,
            consumed: 0,
        }
    }

    /// Total bytes parsed so far.
    pub const fn consumed(&self) -> usize {
        self.consumed
    }

    /// Feeds the next chunk of input.
    pub fn feed(&mut self, mut chunk: &[u8], visitor: &mut dyn Visitor) -> Result<(), ParseError> {
        while !chunk.is_empty() {
            if self.carry_len == 0 {
                // Fast path: lex in place from the chunk.
                let used = run(&mut self.parser, chunk, self.consumed, false, visitor)?;
                self.consumed += used;
                chunk = &chunk[used..];
                if chunk.is_empty() {
                    break;
                }
                // What is left is the start of a token. Keep it for the next chunk.
                if chunk.len() > CARRY {
                    return Err(ParseError::TokenTooLong {
                        offset: self.consumed,
                        limit: CARRY,
                    });
                }
                self.carry[..chunk.len()].copy_from_slice(chunk);
                self.carry_len = chunk.len();
                break;
            }

            // A token is already waiting. Add bytes until it completes.
            let room = CARRY - self.carry_len;
            if room == 0 {
                return Err(ParseError::TokenTooLong {
                    offset: self.consumed,
                    limit: CARRY,
                });
            }
            let n = min(room, chunk.len());
            self.carry[self.carry_len..self.carry_len + n].copy_from_slice(&chunk[..n]);
            self.carry_len += n;
            chunk = &chunk[n..];
            let used = run(
                &mut self.parser,
                &self.carry[..self.carry_len],
                self.consumed,
                false,
                visitor,
            )?;
            self.carry.copy_within(used..self.carry_len, 0);
            self.carry_len -= used;
            self.consumed += used;
        }
        Ok(())
    }

    /// Declares the end of input, and checks that the document is complete.
    pub fn finish(&mut self, visitor: &mut dyn Visitor) -> Result<(), ParseError> {
        if self.carry_len > 0 {
            let used = run(
                &mut self.parser,
                &self.carry[..self.carry_len],
                self.consumed,
                true,
                visitor,
            )?;
            self.consumed += used;
            self.carry_len = 0;
        }
        self.parser.finish().map_err(|f| f.at(self.consumed))
    }
}

impl<const DEPTH: usize, const CARRY: usize> Default for Stream<DEPTH, CARRY> {
    fn default() -> Self {
        Self::new()
    }
}

/// Lexes and parses as many whole tokens as `buf` holds. Returns the bytes used.
/// `base` is the offset of `buf[0]` in the whole stream.
fn run<const DEPTH: usize>(
    parser: &mut Parser<DEPTH>,
    buf: &[u8],
    base: usize,
    eof: bool,
    visitor: &mut dyn Visitor,
) -> Result<usize, ParseError> {
    let mut pos = 0;
    while pos < buf.len() {
        match lex(&buf[pos..], eof) {
            Lex::Token { token, len } => {
                parser.token(token, visitor).map_err(|f| f.at(base + pos))?;
                pos += len;
            }
            Lex::Skip(n) => pos += n,
            Lex::Incomplete => break,
            Lex::Invalid { at } => {
                return Err(ParseError::Lex {
                    offset: base + pos + at,
                });
            }
        }
    }
    Ok(pos)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::Event;
    use std::string::String;
    use std::vec::Vec;

    fn collect<const D: usize, const C: usize>(
        chunks: &[&[u8]],
    ) -> Result<Vec<String>, ParseError> {
        let mut s: Stream<D, C> = Stream::new();
        let mut out = Vec::new();
        {
            let mut v = |e: Event<'_>| out.push(std::format!("{e:?}"));
            for c in chunks {
                s.feed(c, &mut v)?;
            }
            s.finish(&mut v)?;
        }
        Ok(out)
    }

    #[test]
    fn token_split_across_chunks() {
        let whole = collect::<4, 8>(&[b"[true, \"hi\"]"]).unwrap();
        let split = collect::<4, 8>(&[b"[tr", b"ue, \"h", b"i\"]"]).unwrap();
        assert_eq!(whole, split);
        assert_eq!(whole.len(), 4);
    }

    #[test]
    fn one_byte_at_a_time() {
        let doc = br#"{"k": [1, -2.5e3, null, false]}"#;
        let whole = collect::<4, 8>(&[doc]).unwrap();
        let chunks: Vec<&[u8]> = doc.chunks(1).collect();
        assert_eq!(collect::<4, 8>(&chunks).unwrap(), whole);
    }

    #[test]
    fn carry_overflow_is_an_error() {
        let r = collect::<4, 4>(&[b"[\"abcdefgh", b"ijklmnop\"]"]);
        assert!(matches!(r, Err(ParseError::TokenTooLong { limit: 4, .. })));
    }

    #[test]
    fn error_offsets_are_stream_wide() {
        let r = collect::<4, 8>(&[b"[1,", b" 2 3]"]);
        assert_eq!(r.err(), Some(ParseError::Syntax { offset: 6 }));
    }

    #[test]
    fn bad_byte_reports_its_offset() {
        let r = collect::<4, 8>(&[b"[1, @"]);
        assert_eq!(r.err(), Some(ParseError::Lex { offset: 4 }));
    }

    #[test]
    fn truncated_document_is_unexpected_end() {
        let r = collect::<4, 8>(&[b"[1, 2"]);
        assert_eq!(r.err(), Some(ParseError::UnexpectedEnd { offset: 5 }));
    }
}

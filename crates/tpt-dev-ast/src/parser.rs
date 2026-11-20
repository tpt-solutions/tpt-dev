//! The structural parser: a pushdown automaton over tokens that emits events.

use crate::stack::{Container, Stack};
use crate::token::Token;

/// A structural event. Scalar payloads are slices of the input, as written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// An object opened.
    ObjectStart,
    /// An object closed.
    ObjectEnd,
    /// An array opened.
    ArrayStart,
    /// An array closed.
    ArrayEnd,
    /// An object key.
    Key(&'a [u8]),
    /// A string value.
    String(&'a [u8]),
    /// A number value, as written.
    Number(&'a [u8]),
    /// A boolean value.
    Bool(bool),
    /// A null value.
    Null,
}

/// Receives events. Any `FnMut(Event<'_>)` closure implements it.
pub trait Visitor {
    /// Called once for each event, in document order.
    fn event(&mut self, event: Event<'_>);
}

impl<F: FnMut(Event<'_>)> Visitor for F {
    fn event(&mut self, event: Event<'_>) {
        self(event)
    }
}

/// A grammar error, before the parser knows where it happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// A token that the grammar does not allow here.
    Syntax,
    /// A token after the top-level value is complete.
    TrailingData,
    /// Nesting deeper than the stack allows.
    DepthExceeded {
        /// The stack capacity.
        limit: usize,
    },
    /// The input ended before the top-level value was complete.
    UnexpectedEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Expecting any value.
    Value,
    /// After `[`: a value or `]`.
    ArrayFirst,
    /// After `{`: a key or `}`.
    ObjectFirst,
    /// After `,` in an object: a key.
    Key,
    /// After a key: `:`.
    Colon,
    /// After a value inside a container: `,` or the closing bracket.
    AfterValue,
    /// The top-level value is complete.
    Done,
}

/// The structural parser. It keeps at most `DEPTH` open containers on its stack.
pub struct Parser<const DEPTH: usize> {
    stack: Stack<DEPTH>,
    state: State,
}

impl<const DEPTH: usize> Parser<DEPTH> {
    /// Creates a parser ready for the first value.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ast::Parser;
    ///
    /// let _parser: Parser<8> = Parser::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            stack: Stack::new(),
            state: State::Value,
        }
    }

    /// Feeds one token, emitting events to `out`.
    pub(crate) fn token(&mut self, token: Token<'_>, out: &mut dyn Visitor) -> Result<(), Fault> {
        match self.state {
            State::Done => Err(Fault::TrailingData),
            State::Value => self.value(token, out),
            State::ArrayFirst => match token {
                Token::ArrayEnd => self.close(Container::Array, out),
                _ => self.value(token, out),
            },
            State::ObjectFirst => match token {
                Token::ObjectEnd => self.close(Container::Object, out),
                Token::Str(k) => self.key(k, out),
                _ => Err(Fault::Syntax),
            },
            State::Key => match token {
                Token::Str(k) => self.key(k, out),
                _ => Err(Fault::Syntax),
            },
            State::Colon => match token {
                Token::Colon => {
                    self.state = State::Value;
                    Ok(())
                }
                _ => Err(Fault::Syntax),
            },
            State::AfterValue => match (self.stack.top(), token) {
                (Some(Container::Array), Token::Comma) => {
                    self.state = State::Value;
                    Ok(())
                }
                (Some(Container::Object), Token::Comma) => {
                    self.state = State::Key;
                    Ok(())
                }
                (Some(Container::Array), Token::ArrayEnd) => self.close(Container::Array, out),
                (Some(Container::Object), Token::ObjectEnd) => self.close(Container::Object, out),
                _ => Err(Fault::Syntax),
            },
        }
    }

    /// Checks that the document is complete. Call it once the input has ended.
    pub(crate) fn finish(&self) -> Result<(), Fault> {
        match self.state {
            State::Done => Ok(()),
            _ => Err(Fault::UnexpectedEnd),
        }
    }

    fn key(&mut self, key: &[u8], out: &mut dyn Visitor) -> Result<(), Fault> {
        out.event(Event::Key(key));
        self.state = State::Colon;
        Ok(())
    }

    fn value(&mut self, token: Token<'_>, out: &mut dyn Visitor) -> Result<(), Fault> {
        match token {
            Token::ObjectStart => {
                self.open(Container::Object)?;
                out.event(Event::ObjectStart);
                self.state = State::ObjectFirst;
            }
            Token::ArrayStart => {
                self.open(Container::Array)?;
                out.event(Event::ArrayStart);
                self.state = State::ArrayFirst;
            }
            Token::Str(s) => {
                out.event(Event::String(s));
                self.after_scalar();
            }
            Token::Number(n) => {
                out.event(Event::Number(n));
                self.after_scalar();
            }
            Token::True => {
                out.event(Event::Bool(true));
                self.after_scalar();
            }
            Token::False => {
                out.event(Event::Bool(false));
                self.after_scalar();
            }
            Token::Null => {
                out.event(Event::Null);
                self.after_scalar();
            }
            _ => return Err(Fault::Syntax),
        }
        Ok(())
    }

    fn open(&mut self, c: Container) -> Result<(), Fault> {
        self.stack
            .push(c)
            .map_err(|e| Fault::DepthExceeded { limit: e.limit })
    }

    fn close(&mut self, expected: Container, out: &mut dyn Visitor) -> Result<(), Fault> {
        // The caller has checked `top()`, so this pop returns `expected`.
        let _ = self.stack.pop();
        out.event(match expected {
            Container::Array => Event::ArrayEnd,
            Container::Object => Event::ObjectEnd,
        });
        self.after_scalar();
        Ok(())
    }

    fn after_scalar(&mut self) {
        self.state = if self.stack.is_empty() {
            State::Done
        } else {
            State::AfterValue
        };
    }
}

impl<const DEPTH: usize> Default for Parser<DEPTH> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::string::String;
    use std::vec::Vec;

    fn events(tokens: &[Token<'_>]) -> Result<Vec<String>, Fault> {
        let mut p: Parser<4> = Parser::new();
        let mut out: Vec<String> = Vec::new();
        {
            let mut v = |e: Event<'_>| out.push(std::format!("{e:?}"));
            for t in tokens {
                p.token(*t, &mut v)?;
            }
        }
        p.finish()?;
        Ok(out)
    }

    #[test]
    fn simple_object() {
        let got = events(&[
            Token::ObjectStart,
            Token::Str(b"a"),
            Token::Colon,
            Token::Number(b"1"),
            Token::ObjectEnd,
        ])
        .unwrap();
        assert_eq!(
            got,
            ["ObjectStart", "Key([97])", "Number([49])", "ObjectEnd"]
        );
    }

    #[test]
    fn rejects_missing_colon() {
        let r = events(&[Token::ObjectStart, Token::Str(b"a"), Token::Number(b"1")]);
        assert_eq!(r.err(), Some(Fault::Syntax));
    }

    #[test]
    fn rejects_trailing_data() {
        let r = events(&[Token::Null, Token::Null]);
        assert_eq!(r.err(), Some(Fault::TrailingData));
    }

    #[test]
    fn rejects_unclosed_input() {
        let r = events(&[Token::ArrayStart, Token::True]);
        assert_eq!(r.err(), Some(Fault::UnexpectedEnd));
    }

    #[test]
    fn enforces_depth() {
        let r = events(&[
            Token::ArrayStart,
            Token::ArrayStart,
            Token::ArrayStart,
            Token::ArrayStart,
            Token::ArrayStart,
        ]);
        assert_eq!(r.err(), Some(Fault::DepthExceeded { limit: 4 }));
    }
}

#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(any(test, feature = "std"))]
extern crate std;

mod parser;
mod stack;
mod stream;
mod token;

pub use parser::{Event, Fault, Parser, Visitor};
pub use stack::{DepthExceeded, Stack};
pub use stream::{ParseError, Stream};
pub use token::{Lex, Token, lex};

/// Parses a complete in-memory document, reporting events to `visitor`.
///
/// The input is still read in pieces of the stream's carry size, so the
/// same memory bounds apply as for chunked input.
///
/// ```
/// use tpt_dev_ast::{Event, parse_all};
///
/// let mut keys = 0;
/// parse_all::<8, 32>(br#"{"a": [1, 2], "b": null}"#, &mut |e: Event<'_>| {
///     if let Event::Key(_) = e {
///         keys += 1;
///     }
/// })
/// .unwrap();
/// assert_eq!(keys, 2);
/// ```
pub fn parse_all<const DEPTH: usize, const CARRY: usize>(
    input: &[u8],
    visitor: &mut dyn Visitor,
) -> Result<(), ParseError> {
    let mut stream: Stream<DEPTH, CARRY> = Stream::new();
    stream.feed(input, visitor)?;
    stream.finish(visitor)
}

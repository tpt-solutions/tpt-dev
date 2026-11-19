#![allow(clippy::unwrap_used)]
//! A non-JSON lexer driven by the generic `Feed` driver, in any chunking.

use tpt_dev_ast::{Fault, Feed, Lex, Lexer, Step};

/// Words (runs of letters) and single-byte symbols. Tokens borrow the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok<'a> {
    Word(&'a [u8]),
    Sym(u8),
}

struct WordLexer;

impl Lexer for WordLexer {
    type Token<'a> = Tok<'a>;

    fn lex<'a>(&self, input: &'a [u8], eof: bool) -> Step<Tok<'a>> {
        let Some(&first) = input.first() else {
            return Step::Incomplete;
        };
        if first == b' ' {
            return Step::Skip(input.iter().take_while(|b| **b == b' ').count());
        }
        if first.is_ascii_alphabetic() {
            let n = input.iter().take_while(|b| b.is_ascii_alphabetic()).count();
            if n == input.len() && !eof {
                return Step::Incomplete;
            }
            return Step::Token {
                token: Tok::Word(&input[..n]),
                len: n,
            };
        }
        if first.is_ascii_punctuation() {
            return Step::Token {
                token: Tok::Sym(first),
                len: 1,
            };
        }
        Step::Invalid { at: 0 }
    }
}

fn tokens(chunks: &[&[u8]]) -> Result<Vec<String>, tpt_dev_ast::ParseError> {
    let mut feed: Feed<WordLexer, 8> = Feed::new(WordLexer);
    let mut out = Vec::new();
    let mut sink = |t: Tok<'_>| -> Result<(), Fault> {
        out.push(format!("{t:?}"));
        Ok(())
    };
    for c in chunks {
        feed.feed(c, &mut sink)?;
    }
    feed.finish(&mut sink)?;
    Ok(out)
}

#[test]
fn split_words_join_across_chunks() {
    let whole = tokens(&[b"let x = y;"]).unwrap();
    let split = tokens(&[b"le", b"t x = y", b";"]).unwrap();
    assert_eq!(whole, split);
    assert_eq!(whole.len(), 5);
}

#[test]
fn one_byte_at_a_time_matches_whole() {
    let doc = b"ab, cd; e";
    let whole = tokens(&[doc]).unwrap();
    let chunks: Vec<&[u8]> = doc.chunks(1).collect();
    assert_eq!(tokens(&chunks).unwrap(), whole);
}

#[test]
fn word_longer_than_carry_is_an_error() {
    let r = tokens(&[b"abcdefghijk", b"l"]);
    assert!(matches!(
        r,
        Err(tpt_dev_ast::ParseError::TokenTooLong { limit: 8, .. })
    ));
}

#[test]
fn lexer_errors_carry_stream_offsets() {
    let r = tokens(&[b"ab ", b"\x01"]);
    assert_eq!(r.err(), Some(tpt_dev_ast::ParseError::Lex { offset: 3 }));
}

#[test]
fn json_lexer_is_the_default_lexer() {
    use tpt_dev_ast::{JsonLexer, Token};
    assert_eq!(
        JsonLexer.lex(b"null", true),
        Lex::Token {
            token: Token::Null,
            len: 4
        }
    );
}

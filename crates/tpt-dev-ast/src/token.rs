//! The lexer: turns bytes into JSON-like tokens without copying them.

/// A token. String and number payloads are slices of the input, as they
/// appear in the source. Escape sequences in strings are not decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token<'a> {
    /// `{`
    ObjectStart,
    /// `}`
    ObjectEnd,
    /// `[`
    ArrayStart,
    /// `]`
    ArrayEnd,
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// A string, without its quotes.
    Str(&'a [u8]),
    /// A number, as written. It is checked for character set only.
    Number(&'a [u8]),
    /// `true`
    True,
    /// `false`
    False,
    /// `null`
    Null,
}

/// The result of one lexing step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lex<'a> {
    /// A token that spans `len` bytes.
    Token {
        /// The token found.
        token: Token<'a>,
        /// Bytes it occupies in the input.
        len: usize,
    },
    /// `n` bytes of whitespace to skip.
    Skip(usize),
    /// The input ends inside a token. More bytes are needed, unless the input is final.
    Incomplete,
    /// The input cannot start a valid token. `at` is the offset within the input.
    Invalid {
        /// Offset of the bad byte, or of the end of input.
        at: usize,
    },
}

/// Lexes one step from the start of `input`.
///
/// `eof` says that no more input will follow. Numbers and literals at the
/// end of the input are complete only when `eof` is set, so a chunked
/// caller passes `false` until it has seen the last chunk.
///
/// ```
/// use tpt_dev_ast::{Lex, Token, lex};
///
/// assert_eq!(
///     lex(b"true,", false),
///     Lex::Token { token: Token::True, len: 4 }
/// );
/// assert_eq!(lex(b"tr", false), Lex::Incomplete);
/// ```
pub fn lex(input: &[u8], eof: bool) -> Lex<'_> {
    let Some(&first) = input.first() else {
        return Lex::Incomplete;
    };
    match first {
        b' ' | b'\t' | b'\r' | b'\n' => {
            let n = input.iter().take_while(|b| is_ws(**b)).count();
            Lex::Skip(n)
        }
        b'{' => single(Token::ObjectStart),
        b'}' => single(Token::ObjectEnd),
        b'[' => single(Token::ArrayStart),
        b']' => single(Token::ArrayEnd),
        b':' => single(Token::Colon),
        b',' => single(Token::Comma),
        b'"' => lex_string(input, eof),
        b'-' | b'0'..=b'9' => lex_number(input, eof),
        b't' => lex_word(input, b"true", Token::True, eof),
        b'f' => lex_word(input, b"false", Token::False, eof),
        b'n' => lex_word(input, b"null", Token::Null, eof),
        _ => Lex::Invalid { at: 0 },
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn single(token: Token<'_>) -> Lex<'_> {
    Lex::Token { token, len: 1 }
}

fn lex_string(input: &[u8], eof: bool) -> Lex<'_> {
    let mut i = 1;
    while i < input.len() {
        match input[i] {
            b'\\' => {
                // The escaped byte must be present to know where the string ends.
                if i + 1 >= input.len() {
                    break;
                }
                i += 2;
            }
            b'"' => {
                return Lex::Token {
                    token: Token::Str(&input[1..i]),
                    len: i + 1,
                };
            }
            _ => i += 1,
        }
    }
    if eof {
        Lex::Invalid { at: input.len() }
    } else {
        Lex::Incomplete
    }
}

fn lex_number(input: &[u8], eof: bool) -> Lex<'_> {
    let n = input
        .iter()
        .take_while(|b| matches!(**b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
        .count();
    if n == input.len() && !eof {
        // More digits may follow in the next chunk.
        return Lex::Incomplete;
    }
    Lex::Token {
        token: Token::Number(&input[..n]),
        len: n,
    }
}

fn lex_word<'a>(input: &'a [u8], word: &'static [u8], token: Token<'a>, eof: bool) -> Lex<'a> {
    let avail = input.len().min(word.len());
    if input[..avail] != word[..avail] {
        return Lex::Invalid { at: 0 };
    }
    if input.len() < word.len() {
        return if eof {
            Lex::Invalid { at: input.len() }
        } else {
            Lex::Incomplete
        };
    }
    Lex::Token {
        token,
        len: word.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punctuation_and_whitespace() {
        assert_eq!(lex(b"  {", false), Lex::Skip(2));
        assert_eq!(
            lex(b"{", false),
            Lex::Token {
                token: Token::ObjectStart,
                len: 1
            }
        );
        assert_eq!(
            lex(b"]", false),
            Lex::Token {
                token: Token::ArrayEnd,
                len: 1
            }
        );
    }

    #[test]
    fn strings_keep_escapes_raw() {
        let got = lex(br#""a\"b" x"#, false);
        assert_eq!(
            got,
            Lex::Token {
                token: Token::Str(br#"a\"b"#),
                len: 6
            }
        );
    }

    #[test]
    fn string_split_at_end_is_incomplete_until_eof() {
        assert_eq!(lex(b"\"abc", false), Lex::Incomplete);
        assert_eq!(lex(b"\"abc", true), Lex::Invalid { at: 4 });
        assert_eq!(lex(b"\"ab\\", false), Lex::Incomplete);
    }

    #[test]
    fn numbers_end_at_delimiter() {
        assert_eq!(
            lex(b"-12.5e+3,", false),
            Lex::Token {
                token: Token::Number(b"-12.5e+3"),
                len: 8
            }
        );
        // At the end of a chunk the number could continue.
        assert_eq!(lex(b"12", false), Lex::Incomplete);
        assert_eq!(
            lex(b"12", true),
            Lex::Token {
                token: Token::Number(b"12"),
                len: 2
            }
        );
    }

    #[test]
    fn literals_need_all_their_bytes() {
        assert_eq!(lex(b"nul", false), Lex::Incomplete);
        assert_eq!(lex(b"nope", false), Lex::Invalid { at: 0 });
        assert_eq!(lex(b"fals", true), Lex::Invalid { at: 4 });
    }

    #[test]
    fn unknown_byte_is_invalid() {
        assert_eq!(lex(b"@", false), Lex::Invalid { at: 0 });
    }
}

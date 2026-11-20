//! A strict JSON parser (RFC 8259) for `cargo metadata` output.
//!
//! It accepts exactly the JSON grammar: no comments, no trailing commas, no
//! leading zeros, no raw control characters in strings, and no unpaired
//! surrogate escapes. Nesting is limited to [`MAX_DEPTH`] so that hostile input
//! cannot exhaust the stack. Every failure is a [`JsonError`]; the parser does
//! not panic.

use std::fmt;

/// The deepest array or object nesting the parser accepts.
pub const MAX_DEPTH: usize = 128;

/// A parsed JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number, as an `f64`.
    Number(f64),
    /// A string, with escapes decoded.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object, as key-value pairs in input order. Duplicate keys are kept.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The value of `key`, if this is an object. With duplicate keys, the first.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::parse_json;
    ///
    /// let v = parse_json(r#"{"name": "tpt"}"#).unwrap();
    /// assert_eq!(v.get("name").and_then(|n| n.as_str()), Some("tpt"));
    /// assert!(v.get("missing").is_none());
    /// ```
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The string, if this is a string.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::parse_json;
    ///
    /// let v = parse_json(r#""hello""#).unwrap();
    /// assert_eq!(v.as_str(), Some("hello"));
    /// assert_eq!(parse_json("1").unwrap().as_str(), None);
    /// ```
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// The elements, if this is an array.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::parse_json;
    ///
    /// let v = parse_json("[1, 2, 3]").unwrap();
    /// assert_eq!(v.as_array().map(|items| items.len()), Some(3));
    /// ```
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    /// The boolean, if this is `true` or `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::parse_json;
    ///
    /// assert_eq!(parse_json("true").unwrap().as_bool(), Some(true));
    /// assert_eq!(parse_json("1").unwrap().as_bool(), None);
    /// ```
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns `true` if this is `null`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::parse_json;
    ///
    /// assert!(parse_json("null").unwrap().is_null());
    /// assert!(!parse_json("0").unwrap().is_null());
    /// ```
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
}

/// A JSON syntax error, with the byte offset where it was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    /// Byte offset into the input.
    pub offset: usize,
    /// What was wrong.
    pub message: &'static str,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid JSON at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for JsonError {}

/// Parses a complete JSON document. Trailing non-whitespace is an error.
///
/// ```
/// use tpt_dev_dep::{Value, parse_json};
///
/// let v = parse_json(r#"{"name": "serde", "optional": false}"#).unwrap();
/// assert_eq!(v.get("name").and_then(Value::as_str), Some("serde"));
/// assert_eq!(v.get("optional").and_then(Value::as_bool), Some(false));
/// assert!(parse_json("[1,]").is_err());
/// ```
pub fn parse(text: &str) -> Result<Value, JsonError> {
    let mut p = Parser { src: text, pos: 0 };
    p.skip_ws();
    let value = p.value(0)?;
    p.skip_ws();
    if p.pos != text.len() {
        return Err(p.error("unexpected data after the value"));
    }
    Ok(value)
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &'static str) -> JsonError {
        JsonError {
            offset: self.pos,
            message,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect_word(&mut self, word: &'static str) -> Result<(), JsonError> {
        if self.src[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(())
        } else {
            Err(self.error("invalid literal"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, JsonError> {
        match self.peek() {
            None => Err(self.error("unexpected end of input")),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Value::String),
            Some(b't') => self.expect_word("true").map(|()| Value::Bool(true)),
            Some(b'f') => self.expect_word("false").map(|()| Value::Bool(false)),
            Some(b'n') => self.expect_word("null").map(|()| Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("unexpected character")),
        }
    }

    fn enter(&self, depth: usize) -> Result<usize, JsonError> {
        if depth >= MAX_DEPTH {
            Err(self.error("nesting is too deep"))
        } else {
            Ok(depth + 1)
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, JsonError> {
        let depth = self.enter(depth)?;
        self.pos += 1; // '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value(depth)?);
            self.skip_ws();
            match self.bump() {
                Some(b',') => {}
                Some(b']') => return Ok(Value::Array(items)),
                _ => return Err(self.error("expected ',' or ']' in array")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, JsonError> {
        let depth = self.enter(depth)?;
        self.pos += 1; // '{'
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Object(pairs));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a string key in object"));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.bump() != Some(b':') {
                return Err(self.error("expected ':' after object key"));
            }
            self.skip_ws();
            let value = self.value(depth)?;
            pairs.push((key, value));
            self.skip_ws();
            match self.bump() {
                Some(b',') => {}
                Some(b'}') => return Ok(Value::Object(pairs)),
                _ => return Err(self.error("expected ',' or '}' in object")),
            }
        }
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.pos += 1; // opening quote
        let mut out = String::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(self.error("unterminated string"));
            };
            match b {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    out.push(self.escape()?);
                }
                0x00..=0x1F => return Err(self.error("control character in string")),
                _ => {
                    // The input is a `str`, so the next char is valid UTF-8.
                    let Some(c) = self.src[self.pos..].chars().next() else {
                        return Err(self.error("unterminated string"));
                    };
                    out.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }
    }

    fn escape(&mut self) -> Result<char, JsonError> {
        match self.bump() {
            Some(b'"') => Ok('"'),
            Some(b'\\') => Ok('\\'),
            Some(b'/') => Ok('/'),
            Some(b'b') => Ok('\u{8}'),
            Some(b'f') => Ok('\u{c}'),
            Some(b'n') => Ok('\n'),
            Some(b'r') => Ok('\r'),
            Some(b't') => Ok('\t'),
            Some(b'u') => self.unicode_escape(),
            _ => Err(self.error("invalid escape sequence")),
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let mut code = 0u32;
        for _ in 0..4 {
            let digit = match self.bump() {
                Some(b @ b'0'..=b'9') => b - b'0',
                Some(b @ b'a'..=b'f') => b - b'a' + 10,
                Some(b @ b'A'..=b'F') => b - b'A' + 10,
                _ => return Err(self.error("invalid \\u escape")),
            };
            code = code * 16 + u32::from(digit);
        }
        Ok(code)
    }

    fn unicode_escape(&mut self) -> Result<char, JsonError> {
        let high = self.hex4()?;
        if (0xDC00..=0xDFFF).contains(&high) {
            return Err(self.error("unpaired low surrogate"));
        }
        if (0xD800..=0xDBFF).contains(&high) {
            if self.bump() != Some(b'\\') || self.bump() != Some(b'u') {
                return Err(self.error("unpaired high surrogate"));
            }
            let low = self.hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(self.error("unpaired high surrogate"));
            }
            let code = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
            return char::from_u32(code).ok_or_else(|| self.error("invalid code point"));
        }
        char::from_u32(high).ok_or_else(|| self.error("invalid code point"))
    }

    fn number(&mut self) -> Result<Value, JsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.error("invalid number")),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error("invalid number"));
            }
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error("invalid number"));
            }
            self.digits();
        }
        let text = &self.src[start..self.pos];
        text.parse::<f64>()
            .map(Value::Number)
            .map_err(|_| self.error("invalid number"))
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    fn ok(text: &str) -> Value {
        match parse(text) {
            Ok(v) => v,
            Err(e) => panic!("{text:?}: {e}"),
        }
    }

    #[test]
    fn parses_scalars() {
        assert_eq!(ok("null"), Value::Null);
        assert_eq!(ok(" true "), Value::Bool(true));
        assert_eq!(ok("false"), Value::Bool(false));
        assert_eq!(ok("0"), Value::Number(0.0));
        assert_eq!(ok("-12.5e2"), Value::Number(-1250.0));
        assert_eq!(ok("1E+3"), Value::Number(1000.0));
    }

    #[test]
    fn parses_nested_containers() {
        let v = ok(r#"{"a": [1, {"b": null}], "c": []}"#);
        let a = v.get("a").and_then(Value::as_array).unwrap_or(&[]);
        assert_eq!(a.first(), Some(&Value::Number(1.0)));
        assert_eq!(a.get(1).and_then(|x| x.get("b")), Some(&Value::Null));
        assert_eq!(
            v.get("c").and_then(Value::as_array).map(<[Value]>::len),
            Some(0)
        );
        assert_eq!(ok("{}"), Value::Object(vec![]));
    }

    #[test]
    fn decodes_escapes_and_surrogate_pairs() {
        assert_eq!(ok(r#""a\"b\\c\/d\n\t""#).as_str(), Some("a\"b\\c/d\n\t"));
        assert_eq!(ok(r#""é""#).as_str(), Some("é"));
        assert_eq!(ok(r#""😀""#).as_str(), Some("😀"));
        assert_eq!(ok("\"héllo\"").as_str(), Some("héllo"));
    }

    #[test]
    fn rejects_malformed_documents() {
        for text in [
            "",
            "   ",
            "[1,]",
            "{\"a\":1,}",
            "{a:1}",
            "[1 2]",
            "01",
            "1.",
            ".5",
            "-",
            "1e",
            "tru",
            "nulll",
            "\"unterminated",
            "[1] [2]",
            "'single'",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn rejects_bad_escapes() {
        for text in [
            r#""\x""#,
            r#""\u12""#,
            r#""\u12G4""#,
            r#""\ud800""#,
            r#""\ud800x""#,
            r#""\udc00""#,
            r#""\ud800A""#,
            "\"raw\u{1}control\"",
            "\"tab\there\"",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn reports_the_offset_of_the_error() {
        let e = parse("[1, ?]").err();
        assert_eq!(e.map(|e| e.offset), Some(4));
    }

    #[test]
    fn accepts_nesting_at_the_limit_and_rejects_beyond_it() {
        let at_limit = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(parse(&at_limit).is_ok());
        let over = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
        assert!(parse(&over).is_err());
    }

    #[test]
    fn deep_unclosed_nesting_is_an_error_not_a_crash() {
        assert!(parse(&"[".repeat(100_000)).is_err());
        assert!(parse(&"{\"a\":".repeat(100_000)).is_err());
    }
}

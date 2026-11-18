//! Identifier checks and string-literal escaping.

use alloc::string::String;
use core::fmt;

/// A generation error: an invalid identifier, for example.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// What was wrong.
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

const KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

/// Returns `true` if `s` is a valid Rust identifier that is not a keyword.
///
/// ```
/// use tpt_dev_codegen::is_ident;
///
/// assert!(is_ident("sensor_count"));
/// assert!(!is_ident("fn"));
/// assert!(!is_ident("1st"));
/// ```
pub fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    if !chars.all(|c| c == '_' || c.is_ascii_alphanumeric()) {
        return false;
    }
    !KEYWORDS.contains(&s)
}

/// Quotes `s` as a Rust string literal, escaping as needed.
///
/// ```
/// use tpt_dev_codegen::string_literal;
///
/// assert_eq!(string_literal("a\"b\n"), "\"a\\\"b\\n\"");
/// ```
pub fn string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                out.push_str("\\u{");
                out.push_str(&alloc::format!("{:x}", c as u32));
                out.push('}');
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert!(is_ident("_x1"));
        assert!(!is_ident(""));
        assert!(!is_ident("a-b"));
        assert!(!is_ident("match"));
        assert!(!is_ident("ünï"));
    }

    #[test]
    fn literals_escape_control_characters() {
        assert_eq!(string_literal("\u{1}"), "\"\\u{1}\"");
        assert_eq!(string_literal("\\"), "\"\\\\\"");
    }
}

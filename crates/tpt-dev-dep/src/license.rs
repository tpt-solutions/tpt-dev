//! License policy: an allow list checked against SPDX-style expressions.

/// Which licenses are acceptable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    /// Allowed license identifiers, such as `MIT` or `Apache-2.0`.
    pub allow: Vec<String>,
}

impl Policy {
    /// Returns `true` if `expr` is acceptable under this policy.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::Policy;
    ///
    /// let policy = Policy { allow: vec!["MIT".to_string()] };
    /// assert!(policy.accepts("MIT OR Apache-2.0"));
    /// assert!(!policy.accepts("GPL-3.0-only"));
    /// ```
    pub fn accepts(&self, expr: &str) -> bool {
        license_allowed(expr, &self.allow)
    }
}

/// Checks a license expression against an allow list.
///
/// Supports `OR` (or `/`), where one allowed option is enough, and `AND`,
/// where every term must be allowed. `AND` binds tighter than `OR`, as in SPDX.
/// Parentheses group terms. A `WITH` exception is ignored and the base license
/// is checked. A malformed expression is not allowed.
///
/// `MIT OR Apache-2.0` passes when `MIT` is allowed, and `Apache-2.0` alone
/// fails when only `MIT` is allowed.
///
/// ```
/// use tpt_dev_dep::license_allowed;
///
/// let allow = vec!["MIT".to_string(), "BSD-3-Clause".to_string()];
/// assert!(license_allowed("MIT OR Apache-2.0", &allow));
/// assert!(!license_allowed("Apache-2.0", &allow));
/// assert!(license_allowed("(MIT OR Apache-2.0) AND BSD-3-Clause", &allow));
/// ```
pub fn license_allowed(expr: &str, allow: &[String]) -> bool {
    let spaced = expr
        .replace('(', " ( ")
        .replace(')', " ) ")
        .replace('/', " / ");
    let tokens: Vec<Token> = spaced.split_whitespace().map(Token::from_word).collect();
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        allow,
    };
    match parser.or_expr() {
        Some(allowed) if parser.pos == tokens.len() => allowed,
        _ => false,
    }
}

/// One word of a license expression.
#[derive(Debug, PartialEq)]
enum Token<'a> {
    Open,
    Close,
    Or,
    And,
    With,
    Id(&'a str),
}

impl<'a> Token<'a> {
    fn from_word(word: &'a str) -> Self {
        match word {
            "(" => Token::Open,
            ")" => Token::Close,
            "OR" | "/" => Token::Or,
            "AND" => Token::And,
            "WITH" => Token::With,
            id => Token::Id(id),
        }
    }
}

/// Recursive-descent evaluation. Each method returns `None` on a syntax error.
struct Parser<'t, 'a> {
    tokens: &'t [Token<'a>],
    pos: usize,
    allow: &'t [String],
}

impl<'t, 'a> Parser<'t, 'a> {
    fn peek(&self) -> Option<&'t Token<'a>> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&'t Token<'a>> {
        let tok = self.tokens.get(self.pos);
        self.pos += 1;
        tok
    }

    fn or_expr(&mut self) -> Option<bool> {
        let mut value = self.and_expr()?;
        while self.peek() == Some(&Token::Or) {
            self.pos += 1;
            value |= self.and_expr()?;
        }
        Some(value)
    }

    fn and_expr(&mut self) -> Option<bool> {
        let mut value = self.primary()?;
        while self.peek() == Some(&Token::And) {
            self.pos += 1;
            value &= self.primary()?;
        }
        Some(value)
    }

    fn primary(&mut self) -> Option<bool> {
        match self.advance()? {
            Token::Open => {
                let value = self.or_expr()?;
                if self.advance()? != &Token::Close {
                    return None;
                }
                Some(value)
            }
            Token::Id(id) => {
                if self.peek() == Some(&Token::With) {
                    self.pos += 1;
                    // The exception name must follow WITH; it is ignored.
                    match self.advance()? {
                        Token::Id(_) => {}
                        _ => return None,
                    }
                }
                Some(self.allow.iter().any(|a| a == id))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn or_needs_one_allowed_option() {
        assert!(license_allowed("MIT OR Apache-2.0", &allow(&["MIT"])));
        assert!(license_allowed("MIT/Apache-2.0", &allow(&["Apache-2.0"])));
        assert!(!license_allowed("GPL-3.0-only", &allow(&["MIT"])));
    }

    #[test]
    fn and_needs_every_term() {
        assert!(!license_allowed("MIT AND GPL-2.0-only", &allow(&["MIT"])));
        assert!(license_allowed(
            "MIT AND BSD-3-Clause",
            &allow(&["MIT", "BSD-3-Clause"])
        ));
    }

    #[test]
    fn exceptions_use_the_base_license() {
        assert!(license_allowed("MIT WITH LLVM-exception", &allow(&["MIT"])));
    }

    #[test]
    fn empty_expression_is_rejected() {
        assert!(!license_allowed("", &allow(&["MIT"])));
    }

    #[test]
    fn parentheses_group_terms() {
        let a = allow(&["MIT", "BSD-3-Clause"]);
        assert!(license_allowed("(MIT OR Apache-2.0) AND BSD-3-Clause", &a));
        assert!(!license_allowed("(GPL-3.0-only OR Apache-2.0) AND MIT", &a));
        assert!(license_allowed(
            "GPL-3.0-only OR (MIT AND BSD-3-Clause)",
            &a
        ));
        assert!(license_allowed("(MIT)", &a));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        // Parsed as MIT OR (GPL AND MIT): allowed through the first term.
        assert!(license_allowed(
            "MIT OR GPL-3.0-only AND MIT",
            &allow(&["MIT"])
        ));
        // Parsed as (GPL AND MIT) OR Apache: Apache is not allowed, so it fails.
        assert!(!license_allowed(
            "GPL-3.0-only AND MIT OR Apache-2.0",
            &allow(&["MIT"])
        ));
    }

    #[test]
    fn malformed_expressions_are_rejected() {
        let a = allow(&["MIT"]);
        assert!(!license_allowed("(MIT", &a));
        assert!(!license_allowed("MIT)", &a));
        assert!(!license_allowed("MIT OR", &a));
        assert!(!license_allowed("MIT WITH", &a));
        assert!(!license_allowed("OR MIT", &a));
        assert!(!license_allowed("()", &a));
    }
}

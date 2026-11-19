//! License policy: an allow list checked against SPDX-style expressions.

/// Which licenses are acceptable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    /// Allowed license identifiers, such as `MIT` or `Apache-2.0`.
    pub allow: Vec<String>,
}

impl Policy {
    /// Returns `true` if `expr` is acceptable under this policy.
    pub fn accepts(&self, expr: &str) -> bool {
        license_allowed(expr, &self.allow)
    }
}

/// Checks a license expression against an allow list.
///
/// Supports `OR` (or `/`), where one allowed option is enough, and `AND`,
/// where every term must be allowed. A `WITH` exception is ignored and the
/// base license is checked. Parentheses are not supported.
///
/// `MIT OR Apache-2.0` passes when `MIT` is allowed, and `Apache-2.0` alone
/// fails when only `MIT` is allowed.
///
/// ```
/// use tpt_dev_dep::license_allowed;
///
/// let allow = vec!["MIT".to_string()];
/// assert!(license_allowed("MIT OR Apache-2.0", &allow));
/// assert!(!license_allowed("Apache-2.0", &allow));
/// ```
pub fn license_allowed(expr: &str, allow: &[String]) -> bool {
    let normalised = expr.replace('/', " OR ");
    normalised.split(" OR ").any(|alt| {
        alt.split(" AND ").all(|term| {
            let base = term.split(" WITH ").next().unwrap_or(term).trim();
            !base.is_empty() && allow.iter().any(|a| a == base)
        })
    })
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
}

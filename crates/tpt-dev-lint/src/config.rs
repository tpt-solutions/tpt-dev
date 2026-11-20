//! The config file: one `rule = severity` pair per line.
//!
//! Lines starting with `#` and blank lines are ignored.
//!
//! ```text
//! # Allow unwrap in this project, and fail the build on a missing SAFETY comment.
//! no-unwrap = allow
//! safety-comment = deny
//! ```

use std::fmt;

use crate::Severity;

/// The parsed settings: rule name and severity, in file order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    entries: Vec<(String, Severity)>,
}

/// A config file that could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// A line is not `rule = severity`.
    Malformed {
        /// 1-based line number.
        line: usize,
    },
    /// The severity is not `allow`, `warn` or `deny`.
    BadSeverity {
        /// 1-based line number.
        line: usize,
    },
    /// The config names a rule that does not exist.
    UnknownRule(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Malformed { line } => {
                write!(f, "config line {line}: expected `rule = severity`")
            }
            ConfigError::BadSeverity { line } => {
                write!(
                    f,
                    "config line {line}: severity must be allow, warn or deny"
                )
            }
            ConfigError::UnknownRule(name) => write!(f, "config names an unknown rule `{name}`"),
        }
    }
}

impl Config {
    /// Parses config text.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lint::Config;
    ///
    /// let config = Config::parse("no-unwrap = deny\n# comments are skipped\ndoc-example = allow\n").unwrap();
    /// assert_eq!(config.entries().count(), 2);
    ///
    /// assert!(Config::parse("no-unwrap deny").is_err());
    /// ```
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let mut entries = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (name, sev) = line
                .split_once('=')
                .ok_or(ConfigError::Malformed { line: i + 1 })?;
            let name = name.trim();
            if name.is_empty() {
                return Err(ConfigError::Malformed { line: i + 1 });
            }
            let severity =
                Severity::parse(sev.trim()).ok_or(ConfigError::BadSeverity { line: i + 1 })?;
            entries.push((name.to_string(), severity));
        }
        Ok(Self { entries })
    }

    /// The configured `(rule, severity)` pairs, in file order.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lint::{Config, Severity};
    ///
    /// let config = Config::parse("no-unwrap = deny\nno-todo-macros = warn\n").unwrap();
    /// let entries: Vec<(&str, Severity)> = config.entries().collect();
    /// assert_eq!(entries, vec![("no-unwrap", Severity::Deny), ("no-todo-macros", Severity::Warn)]);
    /// ```
    pub fn entries(&self) -> impl Iterator<Item = (&str, Severity)> + '_ {
        self.entries.iter().map(|(n, s)| (n.as_str(), *s))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_pairs_and_skips_comments() {
        let c = Config::parse("# comment\n\nno-unwrap = allow\nsafety-comment=deny\n").unwrap();
        let got: Vec<_> = c.entries().collect();
        assert_eq!(
            got,
            [
                ("no-unwrap", Severity::Allow),
                ("safety-comment", Severity::Deny)
            ]
        );
    }

    #[test]
    fn reports_bad_lines_with_numbers() {
        assert_eq!(
            Config::parse("a = b"),
            Err(ConfigError::BadSeverity { line: 1 })
        );
        assert_eq!(
            Config::parse("\nnoequals"),
            Err(ConfigError::Malformed { line: 2 })
        );
    }
}

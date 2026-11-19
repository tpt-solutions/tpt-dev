//! Project lint rules over parsed Rust source, without compiler plugins.
//!
//! A [`Rule`] walks a file's syntax tree and reports findings. A [`Linter`]
//! runs the enabled rules, each at a severity set by a config file.
//!
//! The analysis is syntactic. It sees names and shapes, not types, so a rule
//! can say that a method named `unwrap` is called, but not that its receiver
//! is an `Option`. The limits are documented in the README.

#![doc = include_str!("../README.md")]

mod config;
mod rules;

use std::fmt;

pub use config::{Config, ConfigError};
pub use rules::{NoTodoMacros, NoUnwrap, SafetyComment, default_rules};

/// How seriously a finding is treated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The rule is off.
    Allow,
    /// Reported, and the run still succeeds.
    Warn,
    /// Reported, and the run fails.
    Deny,
}

impl Severity {
    /// Parses `allow`, `warn` or `deny`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(Severity::Allow),
            "warn" => Some(Severity::Warn),
            "deny" => Some(Severity::Deny),
            _ => None,
        }
    }
}

/// A problem found by a rule, before its severity is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub column: usize,
    /// What is wrong, and what to do.
    pub message: String,
}

/// A finding with its rule and severity, as reported to the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The rule that reported it.
    pub rule: &'static str,
    /// The severity it was reported at.
    pub severity: Severity,
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub column: usize,
    /// The message.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    /// Formats like rustc and Clippy: `severity[rule]: message`, then the location.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Deny => "error",
            Severity::Warn => "warning",
            Severity::Allow => "note",
        };
        write!(
            f,
            "{level}[{}]: {}\n  --> {}:{}",
            self.rule, self.message, self.line, self.column
        )
    }
}

/// A lint rule.
pub trait Rule {
    /// The rule's name, used in the config file and in output.
    fn name(&self) -> &'static str;

    /// Checks a parsed file. `source` is the file's text, for rules that need comments.
    fn check(&self, file: &syn::File, source: &str) -> Vec<Finding>;
}

/// Runs rules at configured severities.
pub struct Linter {
    rules: Vec<(Box<dyn Rule>, Severity)>,
}

impl Linter {
    /// A linter with the given rules, each at its default severity.
    pub fn new(rules: Vec<(Box<dyn Rule>, Severity)>) -> Self {
        Self { rules }
    }

    /// Applies a config: every rule named there gets its configured severity.
    /// Rules not named keep their default. Names that match no rule are an error.
    pub fn configure(&mut self, config: &Config) -> Result<(), ConfigError> {
        for (name, sev) in config.entries() {
            match self.rules.iter_mut().find(|(r, _)| r.name() == name) {
                Some(slot) => slot.1 = sev,
                None => return Err(ConfigError::UnknownRule(name.to_string())),
            }
        }
        Ok(())
    }

    /// Lints one source file. Returns the diagnostics, in source order.
    pub fn lint_source(&self, source: &str) -> Result<Vec<Diagnostic>, String> {
        let file = syn::parse_file(source).map_err(|e| format!("cannot parse: {e}"))?;
        let mut out = Vec::new();
        for (rule, sev) in &self.rules {
            if *sev == Severity::Allow {
                continue;
            }
            for f in rule.check(&file, source) {
                out.push(Diagnostic {
                    rule: rule.name(),
                    severity: *sev,
                    line: f.line,
                    column: f.column,
                    message: f.message,
                });
            }
        }
        out.sort_by_key(|d| (d.line, d.column));
        Ok(out)
    }
}

/// Whether any diagnostic is at `Deny` severity.
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == Severity::Deny)
}

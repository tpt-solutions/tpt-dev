//! Grammar descriptions: rules made of parsing expressions, written as constants.

/// A parsing expression (PEG).
///
/// Sub-expressions are `&'static` references, so a grammar can be a `const`,
/// and its size is fixed at compile time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expr {
    /// Matches these exact bytes.
    Lit(&'static [u8]),
    /// Matches one byte in the inclusive range.
    Range(u8, u8),
    /// Matches each expression in turn.
    Seq(&'static [Expr]),
    /// Matches the first expression that succeeds, and fails if none does.
    Choice(&'static [Expr]),
    /// Matches zero or more repetitions. Stops when a repetition makes no progress.
    Repeat(&'static Expr),
    /// Matches the expression if it can, and matches nothing otherwise.
    Optional(&'static Expr),
    /// Matches the rule at this index in the grammar.
    Call(usize),
}

/// A named rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule {
    /// The rule's name, used in error messages and parse trees.
    pub name: &'static str,
    /// What the rule matches.
    pub body: Expr,
}

/// A set of rules. Rule 0 is the start rule by convention.
///
/// ```
/// use tpt_dev_dsl::{Expr, Grammar, Rule};
///
/// const DIGIT: Expr = Expr::Range(b'0', b'9');
/// const NUMBER: Grammar = Grammar::new(&[Rule {
///     name: "number",
///     body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
/// }]);
/// assert_eq!(NUMBER.rule_index("number"), Some(0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grammar {
    rules: &'static [Rule],
}

impl Grammar {
    /// A grammar from its rules. Panics at compile time if it has no rules, or
    /// if a `Call` names a rule that does not exist.
    pub const fn new(rules: &'static [Rule]) -> Self {
        assert!(!rules.is_empty(), "a grammar needs at least one rule");
        let g = Self { rules };
        let mut i = 0;
        while i < rules.len() {
            check_calls(&rules[i].body, rules.len());
            i += 1;
        }
        g
    }

    /// The rules, in order.
    pub const fn rules(&self) -> &'static [Rule] {
        self.rules
    }

    /// The index of the rule called `name`.
    pub fn rule_index(&self, name: &str) -> Option<usize> {
        self.rules.iter().position(|r| r.name == name)
    }
}

const fn check_calls(e: &Expr, rules: usize) {
    match e {
        Expr::Lit(_) | Expr::Range(_, _) => {}
        Expr::Seq(items) | Expr::Choice(items) => {
            let mut i = 0;
            while i < items.len() {
                check_calls(&items[i], rules);
                i += 1;
            }
        }
        Expr::Repeat(inner) | Expr::Optional(inner) => check_calls(inner, rules),
        Expr::Call(index) => assert!(
            *index < rules,
            "a rule refers to a rule that does not exist"
        ),
    }
}

//! Baseline storage: one line per benchmark, in a plain text format.
//!
//! Each line is `name mean stddev n`, separated by spaces. The name must not
//! contain spaces. Lines starting with `#` are comments.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// The recorded result of one benchmark.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Benchmark name, without spaces.
    pub name: String,
    /// Mean time per iteration.
    pub mean: f64,
    /// Standard deviation of the samples.
    pub stddev: f64,
    /// Number of samples kept after trimming.
    pub n: usize,
}

/// A set of recorded benchmarks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Baseline {
    entries: Vec<Entry>,
}

/// A baseline file that could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// The 1-based line number.
    pub line: usize,
    /// What was wrong with the line.
    pub reason: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}

impl Baseline {
    /// An empty baseline.
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Adds or replaces the entry for `entry.name`.
    pub fn insert(&mut self, entry: Entry) {
        match self.entries.iter_mut().find(|e| e.name == entry.name) {
            Some(slot) => *slot = entry,
            None => self.entries.push(entry),
        }
    }

    /// The entry for `name`, if recorded.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// All entries, in insertion order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Writes the baseline in its text format.
    pub fn to_text(&self) -> String {
        let mut out = String::from("# name mean stddev n\n");
        for e in &self.entries {
            out.push_str(&alloc::format!(
                "{} {} {} {}\n",
                e.name,
                e.mean,
                e.stddev,
                e.n
            ));
        }
        out
    }

    /// Reads the text format produced by [`Baseline::to_text`].
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut entries = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let err = |reason| ParseError {
                line: i + 1,
                reason,
            };
            let mut parts = line.split_whitespace();
            let name = parts.next().ok_or_else(|| err("missing name"))?;
            let mean = parts
                .next()
                .and_then(|s| s.parse::<f64>().ok())
                .ok_or_else(|| err("bad mean"))?;
            let stddev = parts
                .next()
                .and_then(|s| s.parse::<f64>().ok())
                .ok_or_else(|| err("bad stddev"))?;
            let n = parts
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or_else(|| err("bad sample count"))?;
            if parts.next().is_some() {
                return Err(err("too many fields"));
            }
            entries.push(Entry {
                name: String::from(name),
                mean,
                stddev,
                n,
            });
        }
        Ok(Self { entries })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        let mut b = Baseline::new();
        b.insert(Entry {
            name: String::from("parse"),
            mean: 12.5,
            stddev: 0.25,
            n: 30,
        });
        let text = b.to_text();
        assert_eq!(Baseline::parse(&text).unwrap(), b);
    }

    #[test]
    fn bad_lines_are_reported_with_numbers() {
        let e = Baseline::parse("ok 1 0.1 5\nbroken x 0 1\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert_eq!(e.reason, "bad mean");
    }

    #[test]
    fn insert_replaces_by_name() {
        let mut b = Baseline::new();
        let mk = |mean| Entry {
            name: String::from("a"),
            mean,
            stddev: 0.0,
            n: 2,
        };
        b.insert(mk(1.0));
        b.insert(mk(2.0));
        assert_eq!(b.entries().len(), 1);
        assert_eq!(b.get("a").unwrap().mean, 2.0);
    }
}

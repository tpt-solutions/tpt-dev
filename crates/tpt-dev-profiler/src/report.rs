//! Reports: turning samples into per-function counts and flamegraph input.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};

use crate::sampler::Sampler;
use crate::symbols::Symbols;

/// The name given to a program counter that no symbol covers.
pub const UNKNOWN: &str = "[unknown]";

/// Sample counts for one function.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Samples whose innermost frame is in this function.
    pub own: u64,
    /// Samples that have this function anywhere in the stack.
    pub inclusive: u64,
}

/// A summary of a sampler's contents, attributed to functions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    total: u64,
    dropped: u64,
    functions: BTreeMap<String, Counts>,
    folded: BTreeMap<String, u64>,
}

impl Report {
    /// Attributes every sample in `sampler` to a function, using `symbols`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_profiler::{Report, Sampler, Symbols};
    ///
    /// const TABLE: &[(usize, &str)] = &[(0x100, "main"), (0x200, "filter")];
    /// let symbols = Symbols::new(TABLE, 0x300).unwrap();
    /// let sampler: Sampler<8, 4> = Sampler::new();
    /// // Frames are innermost first: filter called from main.
    /// sampler.record(&[0x210, 0x110]);
    /// sampler.record(&[0x210]);
    /// let report = Report::new(&sampler, &symbols);
    /// assert_eq!(report.total(), 2);
    /// ```
    pub fn new<const N: usize, const D: usize>(
        sampler: &Sampler<N, D>,
        symbols: &Symbols<'_>,
    ) -> Self {
        let mut report = Self {
            dropped: sampler.dropped() as u64,
            ..Self::default()
        };
        sampler.for_each(|frames| {
            report.total += 1;
            let names: Vec<&str> = frames
                .iter()
                .map(|&pc| symbols.lookup(pc).unwrap_or(UNKNOWN))
                .collect();
            let Some(leaf) = names.first() else {
                return;
            };
            report.functions.entry(String::from(*leaf)).or_default().own += 1;
            for (i, name) in names.iter().enumerate() {
                if !names[..i].contains(name) {
                    report
                        .functions
                        .entry(String::from(*name))
                        .or_default()
                        .inclusive += 1;
                }
            }
            // Folded stacks run from the outermost frame to the innermost.
            let line = names.iter().rev().copied().collect::<Vec<_>>().join(";");
            *report.folded.entry(line).or_insert(0) += 1;
        });
        report
    }

    /// The number of samples the sampler held when the report was built.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_profiler::{Report, Sampler, Symbols};
    ///
    /// const TABLE: &[(usize, &str)] = &[(0x100, "main"), (0x200, "filter")];
    /// let symbols = Symbols::new(TABLE, 0x300).unwrap();
    /// let sampler: Sampler<8, 4> = Sampler::new();
    /// // Frames are innermost first: filter called from main.
    /// sampler.record(&[0x210, 0x110]);
    /// sampler.record(&[0x210]);
    /// let report = Report::new(&sampler, &symbols);
    /// assert_eq!(report.total(), 2);
    /// ```
    pub fn total(&self) -> u64 {
        self.total
    }

    /// The number of samples the sampler dropped because it was full.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_profiler::{Report, Sampler, Symbols};
    ///
    /// const TABLE: &[(usize, &str)] = &[(0x100, "main")];
    /// let symbols = Symbols::new(TABLE, 0x200).unwrap();
    /// // A buffer of one sample drops the second.
    /// let sampler: Sampler<1, 4> = Sampler::new();
    /// sampler.record(&[0x110]);
    /// sampler.record(&[0x110]);
    /// let report = Report::new(&sampler, &symbols);
    /// assert_eq!(report.dropped(), 1);
    /// ```
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Per-function counts, most self samples first. Ties are sorted by name.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_profiler::{Report, Sampler, Symbols};
    ///
    /// const TABLE: &[(usize, &str)] = &[(0x100, "main"), (0x200, "filter")];
    /// let symbols = Symbols::new(TABLE, 0x300).unwrap();
    /// let sampler: Sampler<8, 4> = Sampler::new();
    /// // Frames are innermost first: filter called from main.
    /// sampler.record(&[0x210, 0x110]);
    /// sampler.record(&[0x210]);
    /// let report = Report::new(&sampler, &symbols);
    /// // filter has two self samples, and appears in both stacks.
    /// assert_eq!(report.functions()[0], ("filter", tpt_dev_profiler::Counts { own: 2, inclusive: 2 }));
    /// ```
    pub fn functions(&self) -> Vec<(&str, Counts)> {
        let mut out: Vec<(&str, Counts)> = self
            .functions
            .iter()
            .map(|(name, counts)| (name.as_str(), *counts))
            .collect();
        out.sort_by(|a, b| b.1.own.cmp(&a.1.own).then(a.0.cmp(b.0)));
        out
    }

    /// Writes a summary table: self and total percentages, and the function name.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_profiler::{Report, Sampler, Symbols};
    ///
    /// const TABLE: &[(usize, &str)] = &[(0x100, "main"), (0x200, "filter")];
    /// let symbols = Symbols::new(TABLE, 0x300).unwrap();
    /// let sampler: Sampler<8, 4> = Sampler::new();
    /// // Frames are innermost first: filter called from main.
    /// sampler.record(&[0x210, 0x110]);
    /// sampler.record(&[0x210]);
    /// let report = Report::new(&sampler, &symbols);
    /// let mut out = String::new();
    /// report.write_text(&mut out).unwrap();
    /// assert!(out.starts_with("samples: 2  dropped: 0"));
    /// ```
    pub fn write_text(&self, out: &mut impl Write) -> fmt::Result {
        writeln!(out, "samples: {}  dropped: {}", self.total, self.dropped)?;
        writeln!(out, "{:>7} {:>7}  function", "self", "total")?;
        for (name, counts) in self.functions() {
            writeln!(
                out,
                "{:>6.1}% {:>6.1}%  {}",
                percent(counts.own, self.total),
                percent(counts.inclusive, self.total),
                name
            )?;
        }
        Ok(())
    }

    /// Writes folded stacks, one `frame;frame;frame count` line per distinct stack.
    ///
    /// This is the input format of `inferno-flamegraph` and `flamegraph.pl`,
    /// which turn it into an SVG flamegraph.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_profiler::{Report, Sampler, Symbols};
    ///
    /// const TABLE: &[(usize, &str)] = &[(0x100, "main"), (0x200, "filter")];
    /// let symbols = Symbols::new(TABLE, 0x300).unwrap();
    /// let sampler: Sampler<8, 4> = Sampler::new();
    /// // Frames are innermost first: filter called from main.
    /// sampler.record(&[0x210, 0x110]);
    /// sampler.record(&[0x210]);
    /// let report = Report::new(&sampler, &symbols);
    /// let mut out = String::new();
    /// report.write_folded(&mut out).unwrap();
    /// assert!(out.contains("main;filter 1"));
    /// ```
    pub fn write_folded(&self, out: &mut impl Write) -> fmt::Result {
        for (stack, count) in &self.folded {
            writeln!(out, "{stack} {count}")?;
        }
        Ok(())
    }
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const TABLE: &[(usize, &str)] = &[(0x100, "main"), (0x200, "sensor_read"), (0x300, "filter")];

    fn sample_run() -> Report {
        let sampler: Sampler<8, 4> = Sampler::new();
        sampler.record(&[0x210, 0x110]); // sensor_read <- main
        sampler.record(&[0x310, 0x210, 0x110]); // filter <- sensor_read <- main
        sampler.record(&[0x120]); // main
        sampler.record(&[0x999]); // unknown
        Report::new(&sampler, &Symbols::new(TABLE, 0x400).unwrap())
    }

    #[test]
    fn counts_self_and_inclusive_samples() {
        let r = sample_run();
        assert_eq!(r.total(), 4);
        assert_eq!(r.dropped(), 0);
        let counts = r.functions();
        let get = |name: &str| counts.iter().find(|(n, _)| *n == name).unwrap().1;
        assert_eq!(
            get("main"),
            Counts {
                own: 1,
                inclusive: 3
            }
        );
        assert_eq!(
            get("sensor_read"),
            Counts {
                own: 1,
                inclusive: 2
            }
        );
        assert_eq!(
            get("filter"),
            Counts {
                own: 1,
                inclusive: 1
            }
        );
        assert_eq!(
            get(UNKNOWN),
            Counts {
                own: 1,
                inclusive: 1
            }
        );
    }

    #[test]
    fn folded_output_is_root_first() {
        let mut out = alloc::string::String::new();
        sample_run().write_folded(&mut out).unwrap();
        assert_eq!(
            out,
            "[unknown] 1\nmain 1\nmain;sensor_read 1\nmain;sensor_read;filter 1\n"
        );
    }

    #[test]
    fn text_report_has_a_header_and_one_line_per_function() {
        let mut out = alloc::string::String::new();
        sample_run().write_text(&mut out).unwrap();
        assert!(out.starts_with("samples: 4  dropped: 0\n"));
        assert!(out.contains("25.0%"));
        assert_eq!(out.lines().count(), 2 + 4);
    }

    #[test]
    fn functions_are_ordered_by_self_samples_then_name() {
        let report = sample_run();
        let names: Vec<&str> = report.functions().iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["[unknown]", "filter", "main", "sensor_read"]);
    }
}

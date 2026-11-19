//! The CI gate: decide whether a new run is a regression against a baseline.

use crate::baseline::Entry;
use crate::stats::{sqrt, summarise, trim_outliers};

/// When a change counts as a regression.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Policy {
    /// A slowdown must be at least this large, in percent, to fail the gate.
    pub max_regression_pct: f64,
    /// A change must have a t statistic at least this large, in magnitude, to be trusted.
    pub critical_t: f64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            max_regression_pct: 10.0,
            critical_t: 2.0,
        }
    }
}

/// The outcome of comparing a run with its baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// Within the policy.
    Ok,
    /// A trusted slowdown beyond the policy. The gate fails.
    Regression {
        /// Relative slowdown, in percent.
        change_pct: f64,
    },
    /// A trusted speedup beyond the policy. Reported, never a failure.
    Improvement {
        /// Relative speedup, in percent (negative).
        change_pct: f64,
    },
    /// A large change that is within noise. Not trusted either way.
    Inconclusive {
        /// Relative change, in percent.
        change_pct: f64,
    },
}

impl Verdict {
    /// Returns `true` if the gate should fail.
    pub fn is_failure(&self) -> bool {
        matches!(self, Verdict::Regression { .. })
    }
}

/// Compares a new run, given as raw samples, with a recorded baseline.
///
/// Outliers are trimmed from the new samples first. Returns `None` if fewer
/// than two samples remain, or if the baseline mean is zero.
///
/// ```
/// use tpt_dev_bench::{Entry, Policy, Verdict, judge};
///
/// let base = Entry { name: "parse".into(), mean: 100.0, stddev: 1.0, n: 20 };
/// let slow = [130.0, 131.0, 129.0, 130.5, 129.5, 130.2];
/// let v = judge(&slow, &base, Policy::default()).unwrap();
/// assert!(v.is_failure());
/// ```
pub fn judge(samples: &[f64], base: &Entry, policy: Policy) -> Option<Verdict> {
    let kept = trim_outliers(samples);
    let cur = summarise(&kept)?;
    if cur.n < 2 || base.n < 2 || base.mean == 0.0 {
        return None;
    }
    let se =
        sqrt(cur.stddev * cur.stddev / cur.n as f64 + base.stddev * base.stddev / base.n as f64);
    let t = if se == 0.0 {
        if cur.mean == base.mean {
            0.0
        } else {
            f64::INFINITY * (cur.mean - base.mean).signum()
        }
    } else {
        (cur.mean - base.mean) / se
    };
    let change_pct = (cur.mean - base.mean) / base.mean * 100.0;
    let trusted = t.abs() >= policy.critical_t;
    Some(if change_pct > policy.max_regression_pct {
        if trusted {
            Verdict::Regression { change_pct }
        } else {
            Verdict::Inconclusive { change_pct }
        }
    } else if change_pct < -policy.max_regression_pct && trusted {
        Verdict::Improvement { change_pct }
    } else {
        Verdict::Ok
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use alloc::string::String;

    fn base() -> Entry {
        Entry {
            name: String::from("b"),
            mean: 100.0,
            stddev: 1.0,
            n: 20,
        }
    }

    #[test]
    fn same_speed_passes() {
        let s = [100.0, 99.5, 100.5, 100.1, 99.9, 100.0];
        assert_eq!(judge(&s, &base(), Policy::default()), Some(Verdict::Ok));
    }

    #[test]
    fn trusted_slowdown_fails_the_gate() {
        let s = [130.0, 131.0, 129.0, 130.5, 129.5, 130.2];
        let v = judge(&s, &base(), Policy::default()).unwrap();
        assert!(v.is_failure());
    }

    #[test]
    fn noisy_slowdown_is_inconclusive_not_failure() {
        // Huge spread, so the mean difference is not trusted.
        let s = [60.0, 200.0, 90.0, 150.0, 115.0, 130.0];
        let v = judge(
            &s,
            &base(),
            Policy {
                max_regression_pct: 10.0,
                critical_t: 2.0,
            },
        )
        .unwrap();
        assert!(matches!(v, Verdict::Inconclusive { .. }) || v == Verdict::Ok);
        assert!(!v.is_failure());
    }

    #[test]
    fn speedup_is_reported_not_failed() {
        let s = [60.0, 59.5, 60.5, 60.1, 59.9, 60.0];
        let v = judge(&s, &base(), Policy::default()).unwrap();
        assert!(matches!(v, Verdict::Improvement { .. }));
        assert!(!v.is_failure());
    }
}

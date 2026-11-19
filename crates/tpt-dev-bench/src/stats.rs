//! Summary statistics, outlier trimming, and Welch's t-test.

use alloc::vec::Vec;

/// Summary of a set of timing samples, in the sample's unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    /// Number of samples.
    pub n: usize,
    /// Arithmetic mean.
    pub mean: f64,
    /// Sample standard deviation (n - 1 in the denominator).
    pub stddev: f64,
    /// Median.
    pub median: f64,
    /// Smallest sample.
    pub min: f64,
    /// Largest sample.
    pub max: f64,
}

/// Square root by Newton's method, since `core` has no `sqrt` for f64.
pub(crate) fn sqrt(x: f64) -> f64 {
    if x <= 0.0 || x.is_nan() {
        return 0.0;
    }
    let mut guess = x;
    for _ in 0..64 {
        let next = 0.5 * (guess + x / guess);
        if (next - guess).abs() <= guess * 1e-15 {
            return next;
        }
        guess = next;
    }
    guess
}

fn sorted(samples: &[f64]) -> Vec<f64> {
    let mut v = samples.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    v
}

/// The median of already-sorted, non-empty data.
fn median_sorted(v: &[f64]) -> f64 {
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

/// Summarises `samples`. Returns `None` when there are none.
///
/// ```
/// use tpt_dev_bench::summarise;
///
/// let s = summarise(&[1.0, 2.0, 3.0, 4.0]).unwrap();
/// assert_eq!(s.n, 4);
/// assert_eq!(s.median, 2.5);
/// ```
pub fn summarise(samples: &[f64]) -> Option<Summary> {
    if samples.is_empty() {
        return None;
    }
    let n = samples.len();
    let mean = samples.iter().sum::<f64>() / n as f64;
    let var = if n > 1 {
        samples.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n as f64 - 1.0)
    } else {
        0.0
    };
    let v = sorted(samples);
    Some(Summary {
        n,
        mean,
        stddev: sqrt(var),
        median: median_sorted(&v),
        min: v[0],
        max: v[n - 1],
    })
}

/// Removes outliers with Tukey's rule: values beyond 1.5 interquartile ranges
/// outside the quartiles. Returns the kept samples, in input order.
///
/// Very small inputs (under four samples) are returned unchanged.
pub fn trim_outliers(samples: &[f64]) -> Vec<f64> {
    if samples.len() < 4 {
        return samples.to_vec();
    }
    let v = sorted(samples);
    let n = v.len();
    let q1 = median_sorted(&v[..n / 2]);
    let q3 = median_sorted(&v[n.div_ceil(2)..]);
    let iqr = q3 - q1;
    let (lo, hi) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
    samples
        .iter()
        .copied()
        .filter(|&x| x >= lo && x <= hi)
        .collect()
}

/// The result of comparing two sets of samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Comparison {
    /// Relative change of the mean, in percent. Positive means slower.
    pub change_pct: f64,
    /// Welch's t statistic. Its magnitude is large when the difference is unlikely to be noise.
    pub t: f64,
}

/// Compares `current` with `baseline` samples, using Welch's t-test.
///
/// Returns `None` if either side has fewer than two samples, or if the baseline mean is zero.
///
/// The t statistic is compared with a critical value of about 2 for large
/// samples. This is an approximation: the exact critical value depends on the
/// degrees of freedom, which are not computed here.
pub fn compare(current: &[f64], baseline: &[f64]) -> Option<Comparison> {
    let c = summarise(current)?;
    let b = summarise(baseline)?;
    if c.n < 2 || b.n < 2 || b.mean == 0.0 {
        return None;
    }
    let se = sqrt(c.stddev * c.stddev / c.n as f64 + b.stddev * b.stddev / b.n as f64);
    let t = if se == 0.0 {
        if c.mean == b.mean {
            0.0
        } else {
            f64::INFINITY * (c.mean - b.mean).signum()
        }
    } else {
        (c.mean - b.mean) / se
    };
    Some(Comparison {
        change_pct: (c.mean - b.mean) / b.mean * 100.0,
        t,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn sqrt_is_accurate() {
        assert!((sqrt(2.0) - core::f64::consts::SQRT_2).abs() < 1e-12);
        assert_eq!(sqrt(0.0), 0.0);
    }

    #[test]
    fn summary_of_known_data() {
        let s = summarise(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).unwrap();
        assert_eq!(s.mean, 5.0);
        assert_eq!(s.median, 4.5);
        assert!((s.stddev - 2.138_089_935).abs() < 1e-6);
        assert_eq!((s.min, s.max), (2.0, 9.0));
    }

    #[test]
    fn outliers_are_removed() {
        let data = [10.0, 10.1, 9.9, 10.0, 10.2, 9.8, 500.0];
        let kept = trim_outliers(&data);
        assert_eq!(kept.len(), 6);
        assert!(!kept.contains(&500.0));
    }

    #[test]
    fn identical_samples_are_not_different() {
        let a = [1.0, 1.1, 0.9, 1.0];
        let c = compare(&a, &a).unwrap();
        assert_eq!(c.change_pct, 0.0);
        assert_eq!(c.t, 0.0);
    }

    #[test]
    fn clear_slowdown_has_large_t() {
        let base = [100.0, 101.0, 99.0, 100.5, 99.5];
        let slow = [130.0, 131.0, 129.0, 130.5, 129.5];
        let c = compare(&slow, &base).unwrap();
        assert!((c.change_pct - 30.0).abs() < 1.0);
        assert!(c.t > 10.0);
    }
}

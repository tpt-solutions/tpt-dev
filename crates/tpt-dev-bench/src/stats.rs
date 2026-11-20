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
///
/// # Examples
///
/// ```
/// use tpt_dev_bench::trim_outliers;
///
/// let kept = trim_outliers(&[10.0, 11.0, 10.5, 11.5, 10.2, 500.0]);
/// assert!(!kept.contains(&500.0));
/// ```
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
    /// Welch–Satterthwaite degrees of freedom for `t`.
    pub df: f64,
}

/// Compares `current` with `baseline` samples, using Welch's t-test.
///
/// Returns `None` if either side has fewer than two samples, or if the baseline mean is zero.
///
/// To decide significance, compare `t` with [`t_critical_95`] at [`Comparison::df`].
///
/// # Examples
///
/// ```
/// use tpt_dev_bench::compare;
///
/// let current = [130.0, 131.0, 129.0, 130.5];
/// let baseline = [100.0, 101.0, 99.0, 100.5];
/// let c = compare(&current, &baseline).unwrap();
/// assert!(c.change_pct > 25.0);
/// ```
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
        df: welch_df(c.stddev, c.n, b.stddev, b.n),
    })
}

/// Welch–Satterthwaite degrees of freedom for two samples summaries, each given
/// as (standard deviation, count), with counts of at least two. Returns
/// `f64::INFINITY` when both standard errors are zero.
pub(crate) fn welch_df(sd_a: f64, n_a: usize, sd_b: f64, n_b: usize) -> f64 {
    let va = sd_a * sd_a / n_a as f64;
    let vb = sd_b * sd_b / n_b as f64;
    let denom = va * va / (n_a as f64 - 1.0) + vb * vb / (n_b as f64 - 1.0);
    if denom == 0.0 {
        return f64::INFINITY;
    }
    (va + vb) * (va + vb) / denom
}

/// Two-sided 95% critical value of Student's t distribution for `df` degrees of
/// freedom.
///
/// Uses a table and rounds `df` down to the nearest tabulated value, so the
/// result is never smaller than the exact value. The gate is therefore more
/// conservative for small samples, never less. Non-finite or sub-1 `df` use
/// the df = 1 value, except `INFINITY`, which uses the normal limit.
///
/// ```
/// use tpt_dev_bench::t_critical_95;
///
/// assert!((t_critical_95(9.0) - 2.262).abs() < 1e-3);
/// assert!((t_critical_95(f64::INFINITY) - 1.960).abs() < 1e-3);
/// ```
pub fn t_critical_95(df: f64) -> f64 {
    const TABLE: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];
    if df == f64::INFINITY {
        return 1.960;
    }
    if !df.is_finite() || df < 1.0 {
        return TABLE[0];
    }
    let d = df as u64;
    if d <= 30 {
        return TABLE[(d - 1) as usize];
    }
    // Beyond 30 degrees of freedom, use anchor points rounded down.
    match d {
        31..=39 => 2.040,
        40..=59 => 2.021,
        60..=119 => 2.000,
        _ => 1.980,
    }
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

    #[test]
    fn critical_value_falls_as_df_grows() {
        let mut prev = f64::INFINITY;
        for df in 1..=200 {
            let v = t_critical_95(f64::from(df));
            assert!(v <= prev, "df {df} gave {v}, above {prev}");
            assert!(v >= 1.960);
            prev = v;
        }
    }

    #[test]
    fn small_samples_need_a_larger_t() {
        // Five samples a side gives df near 8; the normal value (1.96) is too small.
        let a = [100.0, 101.0, 99.0, 100.5, 99.5];
        let b = [100.0, 101.0, 99.0, 100.5, 99.5];
        let c = compare(&a, &b).unwrap();
        assert!(c.df > 1.0 && c.df <= 8.0 + 1e-9, "df {}", c.df);
        assert!(t_critical_95(c.df) > 1.96);
    }
}

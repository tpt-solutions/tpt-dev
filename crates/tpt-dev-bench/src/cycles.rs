//! Measuring closures with a caller-supplied counter. Works in `no_std` with `alloc`.
//!
//! Use this on bare metal: read a Cortex-M DWT `CYCCNT`, a timer, or any other
//! free-running board counter, and pass it to [`measure_with`].

use alloc::vec::Vec;
use core::hint::black_box;

/// A free-running counter that the harness reads before and after each run.
///
/// Implementations return a monotonically increasing count. It may wrap at
/// `u64` (a 32-bit hardware counter should be widened and extended by the
/// implementation). The samples produced by [`measure_with`] are differences of
/// two readings, so they are in counter ticks, that is cycles.
///
/// **Cycles per unit.** A tick is one cycle of the counted clock. To convert a
/// sample to time, divide by the clock frequency in cycles per second, for
/// example by `168` to get microseconds on a 168 MHz core. Keep that factor
/// with the board code, since the harness does not know the clock.
pub trait CycleSource {
    /// Reads the counter. Each call returns the current count in cycles.
    fn now(&mut self) -> u64;
}

/// Runs `f` `warmup` times untimed, then `iters` times timed with `clock`.
/// Returns the cycle count of each timed run.
///
/// The cycle count of each run is `end - start` using wrapping subtraction,
/// so a hardware counter that wraps between the two reads still gives the
/// right difference. `black_box` keeps the optimiser from removing the work.
/// Trim the result with [`crate::trim_outliers`] before summarising.
///
/// # Examples
///
/// ```
/// use tpt_dev_bench::{ManualCycles, measure_with};
///
/// let mut clock = ManualCycles::new(&[0, 5, 10, 17]);
/// let samples = measure_with(&mut clock, 0, 2, || 1 + 1);
/// assert_eq!(samples, vec![5.0, 7.0]);
/// ```
pub fn measure_with<C, R, F>(clock: &mut C, warmup: usize, iters: usize, mut f: F) -> Vec<f64>
where
    C: CycleSource,
    F: FnMut() -> R,
{
    for _ in 0..warmup {
        black_box(f());
    }
    let mut out = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = clock.now();
        black_box(f());
        let end = clock.now();
        out.push(end.wrapping_sub(start) as f64);
    }
    out
}

/// A host counter whose readings the test supplies in advance.
///
/// Each call to [`CycleSource::now`] returns the next reading. After the list
/// runs out, it keeps returning the last reading, so a run with too few
/// readings gives zero-length samples rather than panicking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualCycles {
    readings: Vec<u64>,
    next: usize,
}

impl ManualCycles {
    /// A counter that will return `readings` in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_bench::ManualCycles;
    ///
    /// let clock = ManualCycles::new(&[1, 2, 3]);
    /// assert_eq!(clock.remaining(), 3);
    /// ```
    pub fn new(readings: &[u64]) -> Self {
        Self {
            readings: readings.to_vec(),
            next: 0,
        }
    }

    /// The number of readings not yet returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_bench::{CycleSource, ManualCycles};
    ///
    /// let mut clock = ManualCycles::new(&[1, 2, 3]);
    /// clock.now();
    /// assert_eq!(clock.remaining(), 2);
    /// ```
    pub fn remaining(&self) -> usize {
        self.readings.len().saturating_sub(self.next)
    }
}

impl CycleSource for ManualCycles {
    fn now(&mut self) -> u64 {
        match self.readings.get(self.next) {
            Some(&v) => {
                self.next += 1;
                v
            }
            None => self.readings.last().copied().unwrap_or(0),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::stats::{summarise, trim_outliers};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn deltas_come_from_pairs_of_readings() {
        // Two reads per iteration: (0,100), (100,200), (200,350).
        let mut clock = ManualCycles::new(&[0, 100, 100, 200, 200, 350]);
        let samples = measure_with(&mut clock, 0, 3, || ());
        assert_eq!(samples, [100.0, 100.0, 150.0]);
        assert_eq!(clock.remaining(), 0);
    }

    #[test]
    fn warmup_does_not_read_the_counter() {
        let mut clock = ManualCycles::new(&[10, 20]);
        let samples = measure_with(&mut clock, 5, 1, || ());
        assert_eq!(samples, [10.0]);
    }

    #[test]
    fn wrapping_counter_gives_the_right_delta() {
        let mut clock = ManualCycles::new(&[u64::MAX - 9, 5]);
        let samples = measure_with(&mut clock, 0, 1, || ());
        assert_eq!(samples, [15.0]);
    }

    #[test]
    fn exact_mean_and_stddev_from_known_deltas() {
        // Deltas 90,110,90,110,90,110,90,110 and one outlier of 9000.
        let deltas = [90u64, 110, 90, 110, 90, 110, 90, 110, 9000];
        let mut readings = Vec::new();
        let mut t = 0u64;
        readings.push(t);
        for d in deltas {
            t += d;
            readings.push(t);
            t += 1000;
            readings.push(t);
        }
        let mut clock = ManualCycles::new(&readings);
        let raw = measure_with(&mut clock, 0, deltas.len(), || ());
        assert_eq!(raw.len(), 9);

        // Tukey trimming removes the 9000 outlier: q1 = 90, q3 = 110, so the
        // fences are 60 and 140.
        let kept = trim_outliers(&raw);
        assert_eq!(kept.len(), 8);
        let s = summarise(&kept).unwrap();
        assert!(close(s.mean, 100.0));
        // Sum of squared deviations is 800 over n - 1 = 7.
        assert!(close(s.stddev, (800.0f64 / 7.0).sqrt()));
    }

    #[test]
    fn outlier_is_excluded_from_the_mean() {
        let raw = [100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 9000.0];
        let kept = trim_outliers(&raw);
        let s = summarise(&kept).unwrap();
        assert_eq!(s.n, 7);
        assert!(close(s.mean, 100.0));
        assert!(close(s.stddev, 0.0));
    }
}

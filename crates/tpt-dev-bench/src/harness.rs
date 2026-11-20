//! Measuring closures with the standard clock. Needs the `std` feature.

extern crate std;

use alloc::vec::Vec;
use std::hint::black_box;
use std::time::Instant;

/// Runs `f` `warmup` times untimed, then `iters` times timed. Returns the time of
/// each timed run, in nanoseconds.
///
/// `black_box` keeps the optimiser from removing the work. Results are
/// per-run times, so trim them with [`crate::trim_outliers`] before summarising.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "std")]
/// # fn main() {
/// use tpt_dev_bench::harness::measure;
///
/// let times = measure(2, 10, || (0..100u32).sum::<u32>());
/// assert_eq!(times.len(), 10);
/// # }
/// # #[cfg(not(feature = "std"))]
/// # fn main() {}
/// ```
pub fn measure<R, F>(warmup: usize, iters: usize, mut f: F) -> Vec<f64>
where
    F: FnMut() -> R,
{
    for _ in 0..warmup {
        black_box(f());
    }
    let mut out = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        black_box(f());
        out.push(start.elapsed().as_nanos() as f64);
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn measures_one_time_per_iteration() {
        let times = measure(2, 5, || (0..1000u64).sum::<u64>());
        assert_eq!(times.len(), 5);
        assert!(times.iter().all(|&t| t >= 0.0));
    }
}

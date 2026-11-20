//! A library with contracts. The compiler proves the ones it can and checks
//! the others in debug builds.

use tpt_dev::{ensures, requires};

/// The distance from `lo` up to `hi`, given that `lo <= hi`.
#[requires(lo <= hi)]
#[ensures(result == hi - lo)]
pub fn span(lo: i64, hi: i64) -> i64 {
    let d = hi - lo;
    d
}

#[cfg(test)]
mod tests {
    use super::span;

    #[test]
    fn span_is_the_difference() {
        assert_eq!(span(2, 9), 7);
    }
}

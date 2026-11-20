//! Firmware library built on tpt-dev. Builds for `thumbv7em-none-eabihf`
//! with `cargo build --target thumbv7em-none-eabihf`.

#![no_std]

/// Returns the number of pages a KV cache of `tokens` tokens needs, for a
/// pool that holds `tokens_per_page` tokens per page.
pub fn pages_needed(tokens: usize, tokens_per_page: usize) -> usize {
    if tokens_per_page == 0 {
        return 0;
    }
    tokens.div_ceil(tokens_per_page)
}

#[cfg(test)]
mod tests {
    use super::pages_needed;

    #[test]
    fn rounds_up() {
        assert_eq!(pages_needed(0, 16), 0);
        assert_eq!(pages_needed(17, 16), 2);
        assert_eq!(pages_needed(16, 0), 0);
    }
}

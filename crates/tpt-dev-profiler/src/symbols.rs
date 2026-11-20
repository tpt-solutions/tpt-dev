//! Symbol tables: mapping a program counter to the function that contains it.
//!
//! The caller supplies the table, usually generated from the binary at build
//! time. It is a sorted list of function start addresses and names, plus the
//! address where the code ends. A program counter belongs to the last function
//! that starts at or before it. Addresses at or past the end of the code are
//! unknown, so they are not attributed to the last function.

/// A sorted table of function start addresses, with the end of the code.
#[derive(Clone, Copy, Debug)]
pub struct Symbols<'a> {
    table: &'a [(usize, &'a str)],
    code_end: usize,
}

impl<'a> Symbols<'a> {
    /// A table over `table`, for code that ends before `code_end`.
    ///
    /// Returns `None` if the addresses are not in ascending order.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_profiler::Symbols;
    ///
    /// const TABLE: &[(usize, &str)] = &[(0x100, "main"), (0x200, "filter")];
    /// assert!(Symbols::new(TABLE, 0x300).is_some());
    /// // Addresses must be in ascending order.
    /// assert!(Symbols::new(&[(0x200, "b"), (0x100, "a")], 0x300).is_none());
    /// ```
    pub fn new(table: &'a [(usize, &'a str)], code_end: usize) -> Option<Self> {
        if table.windows(2).all(|w| w[0].0 <= w[1].0) {
            Some(Self { table, code_end })
        } else {
            None
        }
    }

    /// The name of the function that contains `pc`. `None` before the first
    /// function, or at or past the end of the code.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_profiler::Symbols;
    ///
    /// const TABLE: &[(usize, &str)] = &[(0x100, "main"), (0x200, "filter")];
    /// let symbols = Symbols::new(TABLE, 0x300).unwrap();
    /// assert_eq!(symbols.lookup(0x150), Some("main"));
    /// assert_eq!(symbols.lookup(0x250), Some("filter"));
    /// // Past the end of the code is unknown.
    /// assert_eq!(symbols.lookup(0x400), None);
    /// ```
    pub fn lookup(&self, pc: usize) -> Option<&'a str> {
        if pc >= self.code_end {
            return None;
        }
        let after = self.table.partition_point(|&(start, _)| start <= pc);
        after.checked_sub(1).map(|index| self.table[index].1)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const TABLE: &[(usize, &str)] = &[(0x100, "main"), (0x200, "sensor_read"), (0x300, "filter")];
    const CODE_END: usize = 0x400;

    #[test]
    fn a_pc_belongs_to_the_last_function_started_before_it() {
        let s = Symbols::new(TABLE, CODE_END).unwrap();
        assert_eq!(s.lookup(0x100), Some("main"));
        assert_eq!(s.lookup(0x1ff), Some("main"));
        assert_eq!(s.lookup(0x200), Some("sensor_read"));
        assert_eq!(s.lookup(0x3ff), Some("filter"));
    }

    #[test]
    fn a_pc_past_the_end_of_the_code_is_unknown() {
        let s = Symbols::new(TABLE, CODE_END).unwrap();
        assert_eq!(s.lookup(CODE_END), None);
        assert_eq!(s.lookup(0x9999), None);
    }

    #[test]
    fn a_pc_before_the_first_function_is_unknown() {
        let s = Symbols::new(TABLE, CODE_END).unwrap();
        assert_eq!(s.lookup(0x10), None);
    }

    #[test]
    fn an_unsorted_table_is_rejected() {
        let bad: &[(usize, &str)] = &[(0x200, "b"), (0x100, "a")];
        assert!(Symbols::new(bad, CODE_END).is_none());
    }

    #[test]
    fn an_empty_table_knows_nothing() {
        let s = Symbols::new(&[], CODE_END).unwrap();
        assert_eq!(s.lookup(0x100), None);
    }
}

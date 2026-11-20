//! Coverage feedback: the target reports which edges it reached.

use alloc::collections::BTreeSet;

/// The edges a target run reached. A target calls [`Coverage::hit`] at each
/// branch it wants the fuzzer to know about, using a small, fixed edge id.
///
/// ```
/// use tpt_dev_fuzz::Coverage;
///
/// let mut cov = Coverage::new();
/// cov.hit(3);
/// cov.hit(3);
/// assert_eq!(cov.edges().count(), 1);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    edges: BTreeSet<u32>,
}

impl Coverage {
    /// An empty record.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_fuzz::Coverage;
    ///
    /// let cov = Coverage::new();
    /// assert_eq!(cov.edges().count(), 0);
    /// ```
    pub const fn new() -> Self {
        Self {
            edges: BTreeSet::new(),
        }
    }

    /// Records that edge `id` was reached.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_fuzz::Coverage;
    ///
    /// let mut cov = Coverage::new();
    /// cov.hit(7);
    /// assert_eq!(cov.edges().collect::<Vec<u32>>(), vec![7]);
    /// ```
    pub fn hit(&mut self, id: u32) {
        self.edges.insert(id);
    }

    /// The edges reached, in increasing order.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_fuzz::Coverage;
    ///
    /// let mut cov = Coverage::new();
    /// cov.hit(9);
    /// cov.hit(2);
    /// assert_eq!(cov.edges().collect::<Vec<u32>>(), vec![2, 9]);
    /// ```
    pub fn edges(&self) -> impl Iterator<Item = u32> + '_ {
        self.edges.iter().copied()
    }

    /// Clears the record for the next run.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_fuzz::Coverage;
    ///
    /// let mut cov = Coverage::new();
    /// cov.hit(1);
    /// cov.clear();
    /// assert_eq!(cov.edges().count(), 0);
    /// ```
    pub fn clear(&mut self) {
        self.edges.clear();
    }
}

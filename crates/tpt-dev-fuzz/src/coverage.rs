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
    pub const fn new() -> Self {
        Self {
            edges: BTreeSet::new(),
        }
    }

    /// Records that edge `id` was reached.
    pub fn hit(&mut self, id: u32) {
        self.edges.insert(id);
    }

    /// The edges reached, in increasing order.
    pub fn edges(&self) -> impl Iterator<Item = u32> + '_ {
        self.edges.iter().copied()
    }

    /// Clears the record for the next run.
    pub fn clear(&mut self) {
        self.edges.clear();
    }
}

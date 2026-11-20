//! A fixed-capacity stack of open containers.

/// Returned when the nesting depth would exceed the stack's capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DepthExceeded {
    /// The capacity that was exceeded.
    pub limit: usize,
}

/// The kind of an open container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Container {
    Array,
    Object,
}

/// A stack of up to `DEPTH` open containers. It lives inline, with no heap.
///
/// Pushing past `DEPTH` returns an error. The parser never panics on deep input.
///
/// ```
/// use tpt_dev_ast::Stack;
///
/// let mut s: Stack<2> = Stack::new();
/// assert!(s.push_array().is_ok());
/// assert!(s.push_object().is_ok());
/// assert!(s.push_array().is_err());
/// ```
pub struct Stack<const DEPTH: usize> {
    items: [Container; DEPTH],
    len: usize,
}

impl<const DEPTH: usize> Stack<DEPTH> {
    /// Creates an empty stack. Fails to compile if `DEPTH` is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ast::Stack;
    ///
    /// let stack: Stack<4> = Stack::new();
    /// assert!(stack.is_empty());
    /// assert_eq!(stack.depth(), 0);
    /// ```
    pub const fn new() -> Self {
        const { assert!(DEPTH > 0, "the stack needs room for at least one level") };
        Self {
            items: [Container::Array; DEPTH],
            len: 0,
        }
    }

    /// Current nesting depth.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ast::Stack;
    ///
    /// let mut stack: Stack<4> = Stack::new();
    /// stack.push_array().unwrap();
    /// assert_eq!(stack.depth(), 1);
    /// ```
    pub const fn depth(&self) -> usize {
        self.len
    }

    /// Returns `true` at the top level.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ast::Stack;
    ///
    /// let mut stack: Stack<4> = Stack::new();
    /// assert!(stack.is_empty());
    /// stack.push_object().unwrap();
    /// assert!(!stack.is_empty());
    /// ```
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Opens an array.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ast::Stack;
    ///
    /// let mut stack: Stack<1> = Stack::new();
    /// assert!(stack.push_array().is_ok());
    /// assert!(stack.push_array().is_err());
    /// ```
    pub fn push_array(&mut self) -> Result<(), DepthExceeded> {
        self.push(Container::Array)
    }

    /// Opens an object.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_ast::Stack;
    ///
    /// let mut stack: Stack<1> = Stack::new();
    /// assert!(stack.push_object().is_ok());
    /// assert_eq!(stack.depth(), 1);
    /// ```
    pub fn push_object(&mut self) -> Result<(), DepthExceeded> {
        self.push(Container::Object)
    }

    pub(crate) fn push(&mut self, c: Container) -> Result<(), DepthExceeded> {
        if self.len == DEPTH {
            return Err(DepthExceeded { limit: DEPTH });
        }
        self.items[self.len] = c;
        self.len += 1;
        Ok(())
    }

    pub(crate) fn pop(&mut self) -> Option<Container> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(self.items[self.len])
    }

    pub(crate) fn top(&self) -> Option<Container> {
        self.len.checked_sub(1).map(|i| self.items[i])
    }
}

impl<const DEPTH: usize> Default for Stack<DEPTH> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_and_limit() {
        let mut s: Stack<2> = Stack::new();
        assert!(s.is_empty());
        assert_eq!(s.push(Container::Object), Ok(()));
        assert_eq!(s.push(Container::Array), Ok(()));
        assert_eq!(s.push(Container::Array), Err(DepthExceeded { limit: 2 }));
        assert_eq!(s.depth(), 2);
        assert_eq!(s.pop(), Some(Container::Array));
        assert_eq!(s.top(), Some(Container::Object));
        assert_eq!(s.pop(), Some(Container::Object));
        assert_eq!(s.pop(), None);
    }
}

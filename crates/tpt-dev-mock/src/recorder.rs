//! The runtime behind a generated mock: expectations, matching and call counts.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

type Matcher<A> = Box<dyn Fn(&A) -> bool>;
type Action<A, R> = Box<dyn FnMut(A) -> R>;

struct Expectation<A, R> {
    matcher: Matcher<A>,
    action: Action<A, R>,
    times: Option<usize>,
    used: usize,
}

/// Records calls to one method and answers them from expectations.
///
/// Expectations are checked in the order they were added. The first one whose
/// matcher accepts the arguments, and that has calls left, answers the call.
/// A call that no expectation answers panics, since the test did not expect it.
///
/// Each method of a mocked trait has one `Recorder`. Its arguments are
/// passed as a tuple, `()` for none.
pub struct Recorder<A, R> {
    name: &'static str,
    expectations: RefCell<Vec<Expectation<A, R>>>,
    calls: Cell<usize>,
}

impl<A, R> Recorder<A, R> {
    /// A recorder for the method called `name`, used in failure messages.
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            expectations: RefCell::new(Vec::new()),
            calls: Cell::new(0),
        }
    }

    /// Starts a new expectation. It matches every call until `with` is given.
    pub fn expect(&self) -> ExpectBuilder<'_, A, R> {
        ExpectBuilder {
            recorder: self,
            matcher: None,
            times: None,
        }
    }

    /// How many times the method has been called.
    pub fn calls(&self) -> usize {
        self.calls.get()
    }

    /// Answers a call with the first matching expectation that has calls left.
    ///
    /// Panics if the call matches no expectation. A failed expectation is how a
    /// mock fails the test, so the panic is intended.
    #[allow(clippy::panic)]
    pub fn call(&self, args: A) -> R {
        self.calls.set(self.calls.get() + 1);
        let mut expectations = self.expectations.borrow_mut();
        for e in expectations.iter_mut() {
            let available = e.times.is_none_or(|n| e.used < n);
            if available && (e.matcher)(&args) {
                e.used += 1;
                return (e.action)(args);
            }
        }
        drop(expectations);
        panic!("unexpected call to `{}`", self.name);
    }

    /// Panics if an expectation with a call count was not met. This is how a
    /// mock fails the test, so the panic is intended.
    #[allow(clippy::panic)]
    pub fn verify(&self) {
        for e in self.expectations.borrow().iter() {
            if let Some(n) = e.times
                && e.used != n
            {
                panic!(
                    "`{}` was called {} times, expected {}",
                    self.name, e.used, n
                );
            }
        }
    }
}

/// Builds one expectation. Nothing is recorded until `returns` or `returns_with` is called.
pub struct ExpectBuilder<'r, A, R> {
    recorder: &'r Recorder<A, R>,
    matcher: Option<Matcher<A>>,
    times: Option<usize>,
}

impl<A: 'static, R: 'static> ExpectBuilder<'_, A, R> {
    /// Only matches calls whose arguments satisfy `f`.
    pub fn with<F>(mut self, f: F) -> Self
    where
        F: Fn(&A) -> bool + 'static,
    {
        self.matcher = Some(Box::new(f));
        self
    }

    /// Allows exactly `n` matching calls. [`Recorder::verify`] checks the count.
    pub fn times(mut self, n: usize) -> Self {
        self.times = Some(n);
        self
    }

    /// Answers each matching call with `value`, cloned.
    pub fn returns(self, value: R) -> Self
    where
        R: Clone,
    {
        self.returns_with(move |_| value.clone())
    }

    /// Answers each matching call by calling `f` with its arguments.
    pub fn returns_with<F>(self, f: F) -> Self
    where
        F: FnMut(A) -> R + 'static,
    {
        let matcher: Matcher<A> = match self.matcher {
            Some(m) => m,
            None => Box::new(|_| true),
        };
        self.recorder.expectations.borrow_mut().push(Expectation {
            matcher,
            action: Box::new(f),
            times: self.times,
            used: 0,
        });
        // Return a builder that has nothing left to add, so the value is not
        // recorded twice when it is dropped.
        ExpectBuilder {
            recorder: self.recorder,
            matcher: None,
            times: None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn first_matching_expectation_answers() {
        let r: Recorder<(u8,), u16> = Recorder::new("read");
        r.expect().with(|a| a.0 == 1).returns(10);
        r.expect().returns(99);
        assert_eq!(r.call((1,)), 10);
        assert_eq!(r.call((2,)), 99);
        assert_eq!(r.calls(), 2);
    }

    #[test]
    fn counts_limit_and_verify() {
        let r: Recorder<(), u8> = Recorder::new("tick");
        r.expect().times(2).returns(1);
        assert_eq!(r.call(()), 1);
        assert_eq!(r.call(()), 1);
        r.verify();
    }

    #[test]
    #[should_panic(expected = "was called 1 times, expected 2")]
    fn verify_reports_missing_calls() {
        let r: Recorder<(), u8> = Recorder::new("tick");
        r.expect().times(2).returns(1);
        let _ = r.call(());
        r.verify();
    }

    #[test]
    #[should_panic(expected = "unexpected call to `tick`")]
    fn unexpected_call_panics() {
        let r: Recorder<(), u8> = Recorder::new("tick");
        let _ = r.call(());
    }

    #[test]
    fn returns_with_sees_arguments() {
        let r: Recorder<(u32,), u32> = Recorder::new("double");
        r.expect().returns_with(|(x,)| x * 2);
        assert_eq!(r.call((21,)), 42);
    }
}

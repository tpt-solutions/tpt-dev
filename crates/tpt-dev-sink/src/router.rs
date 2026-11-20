//! Routing rules and the router that fans messages out to lanes.

use tpt_dev_lock::spsc::Consumer;

use crate::lane::Lane;

/// A message as it travels from the producer to the router. `topic` selects the route.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Envelope<T> {
    /// Selects which rules match this message.
    pub topic: u16,
    /// The payload.
    pub value: T,
}

/// Sends messages with a matching topic to one lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule {
    /// The topic to match, or `None` to match every topic.
    pub topic: Option<u16>,
    /// Index of the destination lane.
    pub lane: usize,
}

impl Rule {
    fn matches(&self, topic: u16) -> bool {
        self.topic.is_none_or(|t| t == topic)
    }
}

/// A rule names a lane that does not exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleError {
    /// The index of the bad rule in the table.
    pub rule: usize,
}

/// Result of routing one message or one batch of messages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Routed {
    /// Lane deliveries accepted. A message that matches two rules counts twice.
    pub delivered: usize,
    /// Lane deliveries refused by [`Policy::Reject`](crate::Policy::Reject).
    pub rejected: usize,
}

/// Routes messages to `D` lanes using a table of `R` rules.
///
/// The rules are checked in order, and every matching rule delivers a copy.
/// Routing does not allocate, and it uses only the fixed-size lanes and
/// rule table.
///
/// ```
/// use tpt_dev_sink::{Busy, Lane, Policy, Router, Rule, Sink};
///
/// struct Log(u32);
/// impl Sink<u32> for Log {
///     fn send(&mut self, batch: &[u32]) -> Result<(), Busy> {
///         self.0 += batch.len() as u32;
///         Ok(())
///     }
/// }
///
/// let mut radio = Log(0);
/// let mut flash = Log(0);
/// let mut router: Router<u32, 2, 2, 4> = Router::new(
///     [
///         Lane::new(&mut radio, Policy::DropOldest),
///         Lane::new(&mut flash, Policy::DropNewest),
///     ],
///     [
///         Rule { topic: Some(1), lane: 0 },
///         Rule { topic: None, lane: 1 }, // everything is also logged
///     ],
/// ).unwrap();
///
/// router.route(1, 100);
/// router.route(2, 200);
/// router.flush();
/// drop(router);
/// assert_eq!(radio.0, 1);
/// assert_eq!(flash.0, 2);
/// ```
pub struct Router<'s, T: Copy + Default, const D: usize, const R: usize, const B: usize> {
    lanes: [Lane<'s, T, B>; D],
    rules: [Rule; R],
}

impl<'s, T: Copy + Default, const D: usize, const R: usize, const B: usize> Router<'s, T, D, R, B> {
    /// Creates a router. Fails if any rule names a lane index of `D` or more.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sink::{Busy, Lane, Policy, Router, Rule, RuleError, Sink};
    ///
    /// struct Null;
    /// impl Sink<u8> for Null {
    ///     fn send(&mut self, _batch: &[u8]) -> Result<(), Busy> {
    ///         Ok(())
    ///     }
    /// }
    ///
    /// let mut sink = Null;
    /// // Lane 3 does not exist, so the rule is refused and reports its index.
    /// let bad = Router::<u8, 1, 1, 1>::new([Lane::new(&mut sink, Policy::Reject)], [Rule { topic: None, lane: 3 }]);
    /// assert_eq!(bad.err(), Some(RuleError { rule: 0 }));
    /// ```
    pub fn new(lanes: [Lane<'s, T, B>; D], rules: [Rule; R]) -> Result<Self, RuleError> {
        for (i, rule) in rules.iter().enumerate() {
            if rule.lane >= D {
                return Err(RuleError { rule: i });
            }
        }
        Ok(Self { lanes, rules })
    }

    /// Routes one message to every lane whose rule matches `topic`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sink::{Busy, Lane, Policy, Router, Rule, Sink};
    ///
    /// struct Null;
    /// impl Sink<u8> for Null {
    ///     fn send(&mut self, _batch: &[u8]) -> Result<(), Busy> {
    ///         Ok(())
    ///     }
    /// }
    ///
    /// let mut sink = Null;
    /// let mut router: Router<u8, 1, 1, 4> = Router::new(
    ///     [Lane::new(&mut sink, Policy::DropNewest)],
    ///     [Rule { topic: Some(1), lane: 0 }],
    /// ).unwrap();
    /// assert_eq!(router.route(1, 10).delivered, 1);
    /// assert_eq!(router.route(2, 20).delivered, 0);
    /// ```
    pub fn route(&mut self, topic: u16, value: T) -> Routed {
        let mut out = Routed::default();
        for rule in self.rules.iter() {
            if !rule.matches(topic) {
                continue;
            }
            match self.lanes[rule.lane].push(value) {
                Ok(()) => out.delivered += 1,
                Err(_) => out.rejected += 1,
            }
        }
        out
    }

    /// Takes every message waiting in `rx` and routes it. The ingress queue is
    /// usually filled from an interrupt handler through a `tpt-dev-lock` [`Spsc`](tpt_dev_lock::Spsc).
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_lock::Spsc;
    /// use tpt_dev_sink::{Busy, Envelope, Lane, Policy, Router, Rule, Sink};
    ///
    /// struct Log(u32);
    /// impl Sink<u8> for Log {
    ///     fn send(&mut self, batch: &[u8]) -> Result<(), Busy> {
    ///         self.0 += batch.len() as u32;
    ///         Ok(())
    ///     }
    /// }
    ///
    /// let mut log = Log(0);
    /// let mut router: Router<u8, 1, 1, 1> = Router::new(
    ///     [Lane::new(&mut log, Policy::DropNewest)],
    ///     [Rule { topic: None, lane: 0 }],
    /// ).unwrap();
    /// let mut queue: Spsc<Envelope<u8>, 4> = Spsc::new();
    /// let (mut tx, mut rx) = queue.split();
    /// tx.push(Envelope { topic: 1, value: 9 }).unwrap();
    /// tx.push(Envelope { topic: 2, value: 8 }).unwrap();
    /// assert_eq!(router.pump(&mut rx).delivered, 2);
    /// drop(router);
    /// assert_eq!(log.0, 2);
    /// ```
    pub fn pump<const Q: usize>(&mut self, rx: &mut Consumer<'_, Envelope<T>, Q>) -> Routed {
        let mut total = Routed::default();
        while let Some(env) = rx.pop() {
            let r = self.route(env.topic, env.value);
            total.delivered += r.delivered;
            total.rejected += r.rejected;
        }
        total
    }

    /// Sends every partly filled batch. Returns the number of lanes whose sink refused.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sink::{Busy, Lane, Policy, Router, Rule, Sink};
    ///
    /// struct Blocked;
    /// impl Sink<u8> for Blocked {
    ///     fn send(&mut self, _batch: &[u8]) -> Result<(), Busy> {
    ///         Err(Busy)
    ///     }
    /// }
    ///
    /// let mut sink = Blocked;
    /// let mut router: Router<u8, 1, 1, 4> = Router::new(
    ///     [Lane::new(&mut sink, Policy::Reject)],
    ///     [Rule { topic: None, lane: 0 }],
    /// ).unwrap();
    /// router.route(0, 5);
    /// // The one lane's sink refuses its batch.
    /// assert_eq!(router.flush(), 1);
    /// ```
    pub fn flush(&mut self) -> usize {
        let mut busy = 0;
        for lane in self.lanes.iter_mut() {
            if lane.flush().is_err() {
                busy += 1;
            }
        }
        busy
    }

    /// The counters for lane `index`, or `None` if there is no such lane.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sink::{Busy, Lane, Policy, Router, Rule, Sink};
    ///
    /// struct Null;
    /// impl Sink<u8> for Null {
    ///     fn send(&mut self, _batch: &[u8]) -> Result<(), Busy> {
    ///         Ok(())
    ///     }
    /// }
    ///
    /// let mut sink = Null;
    /// let mut router: Router<u8, 1, 1, 2> = Router::new(
    ///     [Lane::new(&mut sink, Policy::DropNewest)],
    ///     [Rule { topic: None, lane: 0 }],
    /// ).unwrap();
    /// router.route(0, 1);
    /// router.route(0, 2);
    /// assert_eq!(router.stats(0).unwrap().sent, 2);
    /// assert!(router.stats(1).is_none());
    /// ```
    pub fn stats(&self, index: usize) -> Option<crate::LaneStats> {
        self.lanes.get(index).map(|l| l.stats())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{Busy, Policy, Sink};

    struct Count(usize);

    impl Sink<u8> for Count {
        fn send(&mut self, batch: &[u8]) -> Result<(), Busy> {
            self.0 += batch.len();
            Ok(())
        }
    }

    #[test]
    fn rejects_rule_with_missing_lane() {
        let mut a = Count(0);
        let lanes: [Lane<u8, 4>; 1] = [Lane::new(&mut a, Policy::DropNewest)];
        let rules = [
            Rule {
                topic: None,
                lane: 0,
            },
            Rule {
                topic: Some(9),
                lane: 3,
            },
        ];
        let err = Router::new(lanes, rules).err();
        assert_eq!(err, Some(RuleError { rule: 1 }));
    }

    #[test]
    fn topic_matching_and_wildcards() {
        let mut a = Count(0);
        let mut b = Count(0);
        let mut router: Router<u8, 2, 2, 1> = Router::new(
            [
                Lane::new(&mut a, Policy::Reject),
                Lane::new(&mut b, Policy::Reject),
            ],
            [
                Rule {
                    topic: Some(5),
                    lane: 0,
                },
                Rule {
                    topic: None,
                    lane: 1,
                },
            ],
        )
        .unwrap();
        assert_eq!(
            router.route(5, 1),
            Routed {
                delivered: 2,
                rejected: 0
            }
        );
        assert_eq!(
            router.route(6, 1),
            Routed {
                delivered: 1,
                rejected: 0
            }
        );
        router.flush();
        assert_eq!(a.0, 1);
        assert_eq!(b.0, 2);
    }

    #[test]
    fn pump_drains_ingress_queue() {
        use tpt_dev_lock::Spsc;
        let mut a = Count(0);
        let mut router: Router<u8, 1, 1, 2> = Router::new(
            [Lane::new(&mut a, Policy::DropNewest)],
            [Rule {
                topic: None,
                lane: 0,
            }],
        )
        .unwrap();
        let mut q: Spsc<Envelope<u8>, 8> = Spsc::new();
        let (mut tx, mut rx) = q.split();
        for v in 0..5u8 {
            assert!(tx.push(Envelope { topic: 0, value: v }).is_ok());
        }
        let r = router.pump(&mut rx);
        assert_eq!(r.delivered, 5);
        router.flush();
        assert_eq!(router.stats(0).map(|s| s.sent), Some(5));
        assert_eq!(a.0, 5);
    }
}

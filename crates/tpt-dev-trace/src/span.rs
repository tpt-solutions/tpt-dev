//! Span creation and recording.

use crate::clock::Clock;
use crate::collector::Collector;
use crate::ids::{Context, IdSource, SpanId, TraceId};
use crate::record::{Kind, Record};

/// Creates spans and events, and records them to a [`Collector`].
///
/// A `Tracer` is cheap to build and borrows the collector, the clock and the
/// ID source. Build one per task or per device.
///
/// ```
/// use tpt_dev_trace::{AtomicIds, Collector, ManualClock, Tracer, Kind};
///
/// let collector: Collector<8> = Collector::new();
/// let clock = ManualClock::new(0);
/// let ids = AtomicIds::new(1);
/// let tracer = Tracer::new(&collector, &clock, &ids);
///
/// {
///     let span = tracer.start_trace("request");
///     clock.advance(5);
///     span.event("cache-miss", 1);
/// } // the span ends here, five ticks after it started
/// assert_eq!(collector.len(), 3);
/// # let _ = Kind::Event;
/// ```
pub struct Tracer<'a, const N: usize> {
    collector: &'a Collector<N>,
    clock: &'a (dyn Clock + Sync),
    ids: &'a (dyn IdSource + Sync),
}

impl<'a, const N: usize> Tracer<'a, N> {
    /// Creates a tracer that records to `collector`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{AtomicIds, Collector, ManualClock, Tracer};
    ///
    /// let collector: Collector<4> = Collector::new();
    /// let clock = ManualClock::new(0);
    /// let ids = AtomicIds::new(1);
    /// let tracer = Tracer::new(&collector, &clock, &ids);
    /// drop(tracer.start_trace("op"));
    /// assert_eq!(collector.len(), 2);
    /// ```
    pub fn new(
        collector: &'a Collector<N>,
        clock: &'a (dyn Clock + Sync),
        ids: &'a (dyn IdSource + Sync),
    ) -> Self {
        Self {
            collector,
            clock,
            ids,
        }
    }

    /// Starts a new trace with a fresh trace ID and a root span.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{AtomicIds, Collector, ManualClock, Tracer};
    ///
    /// let collector: Collector<4> = Collector::new();
    /// let clock = ManualClock::new(0);
    /// let ids = AtomicIds::new(1);
    /// let tracer = Tracer::new(&collector, &clock, &ids);
    /// let span = tracer.start_trace("request");
    /// clock.advance(2);
    /// drop(span);
    /// assert_eq!(collector.len(), 2);
    /// ```
    pub fn start_trace(&self, name: &'static str) -> SpanGuard<'_, 'a, N> {
        let trace = TraceId(self.ids.next_span().0);
        self.open(trace, SpanId::default(), name)
    }

    /// Starts a span as a child of `parent`, which may come from another task
    /// or another device. The parent context is usually decoded from a message.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{AtomicIds, Collector, Context, ManualClock, SpanId, TraceId, Tracer};
    ///
    /// let collector: Collector<4> = Collector::new();
    /// let clock = ManualClock::new(0);
    /// let ids = AtomicIds::new(1);
    /// let tracer = Tracer::new(&collector, &clock, &ids);
    /// let parent = Context { trace: TraceId(5), span: SpanId(6) };
    /// drop(tracer.child_of(parent, "child"));
    /// assert_eq!(collector.len(), 2);
    /// ```
    pub fn child_of(&self, parent: Context, name: &'static str) -> SpanGuard<'_, 'a, N> {
        self.open(parent.trace, parent.span, name)
    }

    fn open(&self, trace: TraceId, parent: SpanId, name: &'static str) -> SpanGuard<'_, 'a, N> {
        let span = self.ids.next_span();
        let start = self.clock.now();
        self.collector.record(Record {
            kind: Kind::SpanStart,
            trace,
            span,
            parent,
            ts: start,
            name,
            value: 0,
        });
        SpanGuard {
            tracer: self,
            trace,
            span,
            name,
            start,
        }
    }
}

/// An open span. It records its end when dropped.
pub struct SpanGuard<'t, 'a, const N: usize> {
    tracer: &'t Tracer<'a, N>,
    trace: TraceId,
    span: SpanId,
    name: &'static str,
    start: u64,
}

impl<const N: usize> SpanGuard<'_, '_, N> {
    /// The context to pass to other tasks or devices, so that their spans
    /// become children of this one.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{AtomicIds, Collector, ManualClock, Tracer};
    ///
    /// let collector: Collector<8> = Collector::new();
    /// let clock = ManualClock::new(0);
    /// let ids = AtomicIds::new(1);
    /// let tracer = Tracer::new(&collector, &clock, &ids);
    /// let span = tracer.start_trace("op");
    /// let child = tracer.child_of(span.context(), "child");
    /// drop(child);
    /// drop(span);
    /// assert_eq!(collector.len(), 4);
    /// ```
    pub const fn context(&self) -> Context {
        Context {
            trace: self.trace,
            span: self.span,
        }
    }

    /// Records a point event inside this span.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{AtomicIds, Collector, ManualClock, Tracer};
    ///
    /// let collector: Collector<4> = Collector::new();
    /// let clock = ManualClock::new(0);
    /// let ids = AtomicIds::new(1);
    /// let tracer = Tracer::new(&collector, &clock, &ids);
    /// let span = tracer.start_trace("op");
    /// span.event("cache-miss", 1);
    /// drop(span);
    /// assert_eq!(collector.len(), 3);
    /// ```
    pub fn event(&self, name: &'static str, value: u64) {
        self.tracer.collector.record(Record {
            kind: Kind::Event,
            trace: self.trace,
            span: self.span,
            parent: SpanId::default(),
            ts: self.tracer.clock.now(),
            name,
            value,
        });
    }
}

impl<const N: usize> Drop for SpanGuard<'_, '_, N> {
    fn drop(&mut self) {
        let now = self.tracer.clock.now();
        self.tracer.collector.record(Record {
            kind: Kind::SpanEnd,
            trace: self.trace,
            span: self.span,
            parent: SpanId::default(),
            ts: now,
            name: self.name,
            value: now.saturating_sub(self.start),
        });
    }
}

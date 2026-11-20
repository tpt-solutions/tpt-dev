//! Fixed-size trace records.

use crate::ids::{Context, SpanId, TraceId};

/// What a [`Record`] describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A span began. `Record::parent` is the parent span.
    SpanStart,
    /// A span ended. `Record::value` is the span's duration in ticks.
    SpanEnd,
    /// A point event inside a span. `Record::value` is its payload.
    Event,
}

/// One trace record. It has a fixed size and holds no heap data.
///
/// `name` must be `&'static str`, usually a string literal. This keeps records
/// `Copy` and avoids allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    /// What this record describes.
    pub kind: Kind,
    /// The trace the record belongs to.
    pub trace: TraceId,
    /// The span the record belongs to.
    pub span: SpanId,
    /// The parent span. Meaningful for `SpanStart` only.
    pub parent: SpanId,
    /// Timestamp, in clock ticks.
    pub ts: u64,
    /// Name of the span or event.
    pub name: &'static str,
    /// Duration for `SpanEnd`, payload for `Event`, zero for `SpanStart`.
    pub value: u64,
}

impl Default for Record {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Record {
    /// A zeroed record, used to fill buffers.
    pub const EMPTY: Self = Self {
        kind: Kind::Event,
        trace: TraceId(0),
        span: SpanId(0),
        parent: SpanId(0),
        ts: 0,
        name: "",
        value: 0,
    };

    /// The context of the span this record belongs to.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{Context, Record, SpanId, TraceId};
    ///
    /// let r = Record { trace: TraceId(9), span: SpanId(4), ..Record::EMPTY };
    /// assert_eq!(r.context(), Context { trace: TraceId(9), span: SpanId(4) });
    /// ```
    pub const fn context(&self) -> Context {
        Context {
            trace: self.trace,
            span: self.span,
        }
    }
}

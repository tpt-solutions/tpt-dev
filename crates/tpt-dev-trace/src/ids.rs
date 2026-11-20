//! Trace and span identifiers, and their wire encoding for propagation.

use crate::sync::{AtomicU32, Ordering};

/// Identifies one end-to-end trace. Shared by every span in the trace,
/// across tasks and devices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraceId(pub u64);

/// Identifies one span within a trace. `SpanId(0)` means "no span".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct SpanId(pub u64);

/// The part of a trace that is passed to another task or device: the trace
/// and the span that new work should be a child of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    /// The trace the work belongs to.
    pub trace: TraceId,
    /// The span that new work should be a child of.
    pub span: SpanId,
}

impl Context {
    /// Size of the encoded form, in bytes.
    pub const ENCODED_LEN: usize = 16;

    /// Encodes the context into 16 bytes, little-endian. The layout is fixed, so
    /// it can be sent in a packet header or stored in a fixed field.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{Context, SpanId, TraceId};
    ///
    /// let ctx = Context { trace: TraceId(1), span: SpanId(2) };
    /// let bytes = ctx.encode();
    /// assert_eq!(bytes.len(), Context::ENCODED_LEN);
    /// assert_eq!(bytes[0], 1);
    /// assert_eq!(bytes[8], 2);
    /// ```
    pub const fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let t = self.trace.0.to_le_bytes();
        let s = self.span.0.to_le_bytes();
        [
            t[0], t[1], t[2], t[3], t[4], t[5], t[6], t[7], //
            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
        ]
    }

    /// Decodes a context produced by [`Context::encode`].
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{Context, SpanId, TraceId};
    ///
    /// let ctx = Context::decode([1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]);
    /// assert_eq!(ctx.trace, TraceId(1));
    /// assert_eq!(ctx.span, SpanId(2));
    /// ```
    pub const fn decode(bytes: [u8; Self::ENCODED_LEN]) -> Self {
        let mut t = [0u8; 8];
        let mut s = [0u8; 8];
        let mut i = 0;
        while i < 8 {
            t[i] = bytes[i];
            s[i] = bytes[8 + i];
            i += 1;
        }
        Self {
            trace: TraceId(u64::from_le_bytes(t)),
            span: SpanId(u64::from_le_bytes(s)),
        }
    }
}

/// Produces new span identifiers. Implement it when the device has its own
/// source of unique IDs.
pub trait IdSource {
    /// Returns a span ID that no other span in the system uses.
    fn next_span(&self) -> SpanId;
}

/// Span IDs from a 32-bit atomic counter, with a 32-bit node number in the top half.
///
/// Give each device a different `node` so that IDs from different devices
/// do not collide. The counter is 32 bits, so it wraps after about 4.3
/// billion spans on one device. It uses 32-bit atomics because many
/// microcontrollers, including Cortex-M4 and M7, have no 64-bit atomics.
///
/// ```
/// use tpt_dev_trace::{AtomicIds, IdSource};
///
/// let ids = AtomicIds::new(3);
/// let a = ids.next_span();
/// let b = ids.next_span();
/// assert_ne!(a, b);
/// assert_eq!(a.0 >> 32, 3);
/// ```
pub struct AtomicIds {
    prefix: u64,
    counter: AtomicU32,
}

impl AtomicIds {
    /// Creates a generator for device `node`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{AtomicIds, IdSource};
    ///
    /// let ids = AtomicIds::new(0x2A);
    /// let span = ids.next_span();
    /// assert_eq!(span.0 >> 32, 0x2A);
    /// ```
    #[cfg(not(loom))]
    pub const fn new(node: u32) -> Self {
        Self {
            prefix: (node as u64) << 32,
            counter: AtomicU32::new(1),
        }
    }

    /// Creates a generator for device `node`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_trace::{AtomicIds, IdSource};
    ///
    /// let ids = AtomicIds::new(0x2A);
    /// let span = ids.next_span();
    /// assert_eq!(span.0 >> 32, 0x2A);
    /// ```
    #[cfg(loom)]
    pub fn new(node: u32) -> Self {
        Self {
            prefix: (node as u64) << 32,
            counter: AtomicU32::new(1),
        }
    }
}

impl IdSource for AtomicIds {
    fn next_span(&self) -> SpanId {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        SpanId(self.prefix | u64::from(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_round_trips() {
        let ctx = Context {
            trace: TraceId(0x0102_0304_0506_0708),
            span: SpanId(0xDEAD_BEEF_0000_0001),
        };
        let bytes = ctx.encode();
        // Little-endian: the least significant byte comes first.
        assert_eq!(bytes[0], 0x08);
        assert_eq!(bytes[8], 0x01);
        assert_eq!(Context::decode(bytes), ctx);
    }

    #[test]
    fn span_ids_are_unique_and_tagged_with_node() {
        let ids = AtomicIds::new(0xAB);
        let first = ids.next_span();
        let second = ids.next_span();
        assert_ne!(first, second);
        assert_eq!(first.0 >> 32, 0xAB);
        assert_eq!(first.0 & 0xFFFF_FFFF, 1);
    }
}

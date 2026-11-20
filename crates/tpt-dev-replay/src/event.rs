//! Log events and their fixed 16-byte encoding.

use tpt_dev_trace::{Kind, Record, SpanId, TraceId};

/// One thing that happened in a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The scheduler polled task `slot`.
    Schedule {
        /// The executor slot polled.
        slot: u8,
    },
    /// An interrupt woke task `slot`.
    Interrupt {
        /// The executor slot woken.
        slot: u8,
    },
    /// A task read `value` from input `channel`.
    Input {
        /// Which input was read.
        channel: u8,
        /// The value read.
        value: u64,
    },
}

impl Event {
    /// Size of the encoding, in bytes.
    pub const ENCODED_LEN: usize = 16;

    /// The fixed 16-byte encoding: a tag, a one-byte field, six unused bytes,
    /// then the 64-bit value (little-endian).
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::Event;
    ///
    /// let event = Event::Input { channel: 2, value: 5 };
    /// let bytes = event.encode();
    /// assert_eq!(bytes.len(), Event::ENCODED_LEN);
    /// assert_eq!(Event::decode(bytes), Some(event));
    /// ```
    pub const fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut out = [0u8; Self::ENCODED_LEN];
        match *self {
            Event::Schedule { slot } => {
                out[0] = 1;
                out[1] = slot;
            }
            Event::Interrupt { slot } => {
                out[0] = 2;
                out[1] = slot;
            }
            Event::Input { channel, value } => {
                out[0] = 3;
                out[1] = channel;
                let v = value.to_le_bytes();
                let mut i = 0;
                while i < 8 {
                    out[8 + i] = v[i];
                    i += 1;
                }
            }
        }
        out
    }

    /// Decodes a 16-byte encoding. Returns `None` for an unknown tag.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::Event;
    ///
    /// let mut bytes = [0u8; Event::ENCODED_LEN];
    /// bytes[0] = 1; // tag 1 is a schedule step
    /// bytes[1] = 3; // slot 3
    /// assert_eq!(Event::decode(bytes), Some(Event::Schedule { slot: 3 }));
    ///
    /// // An unknown tag is not an event.
    /// assert_eq!(Event::decode([9; Event::ENCODED_LEN]), None);
    /// ```
    pub const fn decode(bytes: [u8; Self::ENCODED_LEN]) -> Option<Self> {
        let mut v = [0u8; 8];
        let mut i = 0;
        while i < 8 {
            v[i] = bytes[8 + i];
            i += 1;
        }
        let value = u64::from_le_bytes(v);
        match bytes[0] {
            1 => Some(Event::Schedule { slot: bytes[1] }),
            2 => Some(Event::Interrupt { slot: bytes[1] }),
            3 => Some(Event::Input {
                channel: bytes[1],
                value,
            }),
            _ => None,
        }
    }

    /// The event as a `tpt-dev-trace` record at timestamp `ts`, so a run can be
    /// exported through the trace pipeline. The span is the schedule step's
    /// slot, or zero for other events.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_replay::Event;
    ///
    /// let record = Event::Input { channel: 1, value: 42 }.to_trace_record(5);
    /// assert_eq!(record.name, "replay.input");
    /// assert_eq!(record.value, 42);
    /// assert_eq!(record.ts, 5);
    /// ```
    pub fn to_trace_record(&self, ts: u64) -> Record {
        let (name, span, value) = match *self {
            Event::Schedule { slot } => ("replay.schedule", u64::from(slot), 0),
            Event::Interrupt { slot } => ("replay.interrupt", u64::from(slot), 0),
            Event::Input { channel, value } => ("replay.input", u64::from(channel), value),
        };
        Record {
            kind: Kind::Event,
            trace: TraceId(0),
            span: SpanId(span),
            parent: SpanId(0),
            ts,
            name,
            value,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn encoding_round_trips() {
        for e in [
            Event::Schedule { slot: 7 },
            Event::Interrupt { slot: 31 },
            Event::Input {
                channel: 2,
                value: 0xDEAD_BEEF_0102_0304,
            },
        ] {
            assert_eq!(Event::decode(e.encode()), Some(e));
        }
        assert_eq!(Event::decode([9; 16]), None);
    }

    #[test]
    fn trace_record_carries_the_value() {
        let r = Event::Input {
            channel: 1,
            value: 42,
        }
        .to_trace_record(5);
        assert_eq!(r.name, "replay.input");
        assert_eq!(r.value, 42);
        assert_eq!(r.ts, 5);
    }
}

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

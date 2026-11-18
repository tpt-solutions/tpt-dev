#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(any(test, feature = "std"))]
extern crate std;

mod clock;
mod collector;
mod ids;
mod record;
mod ring;
mod span;
mod sync;

pub use clock::Clock;
#[cfg(any(loom, target_has_atomic = "64"))]
pub use clock::ManualClock;
pub use collector::{Collector, Sink};
pub use ids::{AtomicIds, Context, IdSource, SpanId, TraceId};
pub use record::{Kind, Record};
pub use ring::Ring;
pub use span::{SpanGuard, Tracer};

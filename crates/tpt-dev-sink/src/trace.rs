//! Integration with `tpt-dev-trace`: a lane sink that exports trace records.

use tpt_dev_trace::{Record, Sink as TraceExporter};

use crate::lane::{Busy, Sink};

/// Adapts a `tpt-dev-trace` exporter so it can be the sink of a lane.
///
/// Trace records then pass through the same batching and back-pressure
/// policy as other telemetry. The trace exporter never refuses a batch, so
/// the lane never sees [`Busy`] from this adapter.
///
/// ```
/// use tpt_dev_sink::{Lane, Policy, TraceSink};
/// use tpt_dev_trace::{Record, Sink as TraceExporter};
///
/// struct Count(usize);
/// impl TraceExporter for Count {
///     fn export(&mut self, records: &[Record]) {
///         self.0 += records.len();
///     }
/// }
///
/// let mut inner = TraceSink(Count(0));
/// let mut lane: Lane<Record, 4> = Lane::new(&mut inner, Policy::DropNewest);
/// lane.push(Record::EMPTY).unwrap();
/// lane.flush().unwrap();
/// assert_eq!(inner.0 .0, 1);
/// ```
pub struct TraceSink<E>(pub E);

impl<E: TraceExporter> Sink<Record> for TraceSink<E> {
    fn send(&mut self, batch: &[Record]) -> Result<(), Busy> {
        self.0.export(batch);
        Ok(())
    }
}

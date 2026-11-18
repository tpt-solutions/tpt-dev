# tpt-dev-trace

Zero-allocation distributed tracing for embedded systems.

- **Fixed-size records:** a `Record` is plain `Copy` data. Names are `&'static str`, so nothing is allocated.
- **Ring buffer collector:** a bounded ring keeps the newest records. Overflow is counted in `dropped()`, not blocked on.
- **Propagation:** a `Context` (trace ID and span ID) encodes to 16 fixed bytes, so it can travel in a packet header.
- **Timestamp abstraction:** `Clock` is implemented for hardware counters, for closures, and for `ManualClock` in tests.
- **Pluggable export:** implement `Sink` for a UART, flash log or socket. `Collector::flush` exports in batches.
- **Model-checked:** `loom` tests cover concurrent recording.

## Example

```rust
use tpt_dev_trace::{AtomicIds, Clock, Collector, Context, Record, Sink, Tracer};

struct Uart;

impl Sink for Uart {
    fn export(&mut self, records: &[Record]) {
        for r in records {
            // write `r` to the serial port here
            let _ = r;
        }
    }
}

// Ticks from the device's cycle counter.
struct Cycles;
impl Clock for Cycles {
    fn now(&self) -> u64 {
        0 // read DWT->CYCCNT on real hardware
    }
}

let collector: Collector<64> = Collector::new();
let ids = AtomicIds::new(1);
let clock = Cycles;
let tracer = Tracer::new(&collector, &clock, &ids);

// Device A starts a trace and sends the context to device B.
let span = tracer.start_trace("sample");
let wire: [u8; Context::ENCODED_LEN] = span.context().encode();

// Device B decodes it and makes its spans children of A's span.
let parent = Context::decode(wire);
let child = tracer.child_of(parent, "filter");
child.event("saturated", 1);
drop(child);
drop(span);

collector.flush(&mut Uart);
```

## Scope

- The collector uses a spin lock (`tpt-dev-lock`). Do not record from an interrupt handler while thread context records to the same collector. The interrupted code could hold the lock and never release it.
- Span IDs are unique only if each device has a different `node` number in `AtomicIds`.

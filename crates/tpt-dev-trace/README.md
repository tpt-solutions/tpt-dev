# tpt-dev-trace

Zero-allocation distributed tracing for embedded systems.

## Overview

`tpt-dev-trace` records spans and events as fixed-size, `Copy` records in a bounded ring buffer. Nothing is allocated: names are `&'static str`, and the collector overwrites its oldest record when it is full and counts the loss. A trace context (trace ID and span ID) encodes to 16 fixed bytes, so it can travel in a packet header and let spans on another device become children of a span on this one.

It is for firmware and other `no_std` code that needs to see where time went across tasks or across devices, without a heap or a logging framework. Timestamps come from a `Clock` you implement over a hardware counter. Export goes through a `Sink` you implement for a UART, flash log or socket.

The collector uses the spin mutex from [`tpt-dev-lock`](https://docs.rs/tpt-dev-lock). [`tpt-dev-async`](https://docs.rs/tpt-dev-async) uses the `Clock` trait for delays, [`tpt-dev-sink`](https://docs.rs/tpt-dev-sink) can route trace records through its lanes, and [`tpt-dev-replay`](https://docs.rs/tpt-dev-replay) converts replay events into trace records.

## Features

- **Fixed-size records:** a `Record` is plain `Copy` data with a `Kind` (`SpanStart`, `SpanEnd`, `Event`), IDs, a timestamp, a static name and a `u64` value.
- **Ring-buffer collector:** a bounded ring keeps the newest records. Overflow is counted in `dropped()`, never blocked on.
- **Context propagation:** `Context` encodes to and decodes from 16 little-endian bytes (`Context::ENCODED_LEN`).
- **Span guards:** `Tracer::start_trace` and `Tracer::child_of` return a `SpanGuard` that records the span end, with its duration, when dropped.
- **Timestamp abstraction:** `Clock` is implemented for any `Fn() -> u64` closure. `ManualClock` is available for tests on targets with 64-bit atomics.
- **Pluggable export:** implement `Sink` and call `Collector::flush`, which exports in batches of 16 without holding the lock during export.
- **Model-checked:** the loom test in `tests/loom.rs` covers concurrent recording.

## Installation

```sh
cargo add tpt-dev-trace
```

The crate is `#![no_std]` and needs no allocator. It is not published yet; until then, depend on it by path or git.

| Feature | Enables | Default |
|---|---|---|
| `std` | links `std` (used for tests and host tooling) | off |

The CI builds the workspace for `thumbv7em-none-eabihf`. Span IDs use a 32-bit atomic, because that target has no 64-bit atomics. `ManualClock` is compiled only where the target has 64-bit atomics, so on Cortex-M4 and M7 implement `Clock` over the hardware timer instead.

## Quick start

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

// Two span starts, one event and two span ends.
assert_eq!(collector.len(), 5);
collector.flush(&mut Uart);
assert!(collector.is_empty());
```

## Design notes and guarantees

- **No allocation.** Records, the ring and the flush batch are fixed arrays. `Collector::new` and `Ring::new` are `const fn`, and `Ring::new` fails to compile when `N` is zero.
- **Recording is one short critical section.** `Collector::record` takes the level-1 spin lock for a single ring push. `flush` takes it once per batch of 16, and releases it before calling the `Sink`, so recording can continue during export.
- **Loss is visible.** When the ring is full, the oldest record is overwritten and `dropped()` goes up (saturating at `u64::MAX`).
- **Span durations.** `SpanEnd` records carry the duration in ticks, computed with saturating subtraction, so a clock that steps back gives 0 rather than a wrap.
- **Span IDs.** `AtomicIds` puts a 32-bit node number in the top half and a 32-bit counter in the bottom half. IDs are unique only if each device uses a different `node`. The counter wraps after about 4.3 billion spans on one device. A trace ID is taken from a fresh span ID.
- **Custom ID sources.** Implement `IdSource` if the device has its own source of unique IDs.

## Limitations and scope

- **Do not record from an interrupt handler** while thread context records to the same collector. The interrupted code could hold the spin lock, and the handler would spin forever.
- **Names must be `&'static str`.** This keeps records `Copy`. Dynamic names are not supported.
- **`Tracer` needs `Sync` clocks and ID sources.** It stores `&dyn Clock + Sync` and `&dyn IdSource + Sync`.
- **No sampling, no filtering, no wire format for records.** Only the 16-byte `Context` has a defined encoding. Encoding records for export is up to your `Sink`.
- **The clock must not go backwards.** Timestamps are whatever `Clock::now` returns; the crate does not check them.
- **Overflowing records are lost.** Size `N` for the longest gap between flushes.

## Related crates

- [`tpt-dev-lock`](https://docs.rs/tpt-dev-lock): provides the spin mutex used by `Collector` (dependency).
- [`tpt-dev-async`](https://docs.rs/tpt-dev-async): uses the `Clock` trait for `Delay` (depends on this crate).
- [`tpt-dev-sink`](https://docs.rs/tpt-dev-sink): `TraceSink` adapts a trace `Sink` to a telemetry lane (depends on this crate).
- [`tpt-dev-replay`](https://docs.rs/tpt-dev-replay): `Event::to_trace_record` produces `Record`s (depends on this crate).

## Status and MSRV

Pre-release (0.0.0, not yet published). The minimum supported Rust version is 1.98 (`rust-version` in `Cargo.toml`).

## License

Licensed under either of MIT or Apache-2.0, at your option.

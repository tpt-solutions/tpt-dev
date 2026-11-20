# tpt-dev-sink

Zero-allocation, lock-free edge telemetry router for microcontrollers.

## Overview

`tpt-dev-sink` moves telemetry messages from producers to destinations such as a UART, a radio or a flash log, without a heap. Each destination is a `Lane`: a `Sink` plus a fixed array of `B` messages and a back-pressure `Policy`. A `Router` holds a fixed table of `Rule`s that match a topic (or every topic) and name a lane. A message that matches several rules is copied to each lane.

It is for edge firmware where an interrupt handler produces data and the main loop ships it out in batches. The ingress path is a [`tpt-dev-lock`](https://docs.rs/tpt-dev-lock) `Spsc` queue, which an interrupt handler can fill without locking. [`tpt-dev-trace`](https://docs.rs/tpt-dev-trace) records can use the same lanes through `TraceSink`.

## Features

- **Heap-free batching:** each lane has a fixed array of `B` messages and sends a whole batch at once.
- **Static routing table:** `Rule`s match a topic or all topics and name a lane. `Router::new` checks that every rule names an existing lane.
- **Explicit back-pressure:** when a sink is busy and a batch is full, the lane applies a `Policy`: `DropNewest`, `DropOldest` or `Reject`.
- **Lock-free ingress:** `Router::pump` drains a `tpt-dev-lock` `Spsc` consumer of `Envelope`s.
- **Counters:** `LaneStats` reports messages sent, messages dropped and messages pending, per lane.
- **Trace integration:** `TraceSink` adapts a `tpt-dev-trace` exporter so trace records pass through a lane.

## Installation

```sh
cargo add tpt-dev-sink
```

The crate is `#![no_std]` and needs no allocator. It is not published yet; until then, depend on it by path or git.

| Feature | Enables | Default |
|---|---|---|
| `std` | links `std` (used for tests and host tooling) | off |

The CI builds the workspace for `thumbv7em-none-eabihf`.

## Quick start

```rust
use tpt_dev_lock::Spsc;
use tpt_dev_sink::{Busy, Envelope, Lane, Policy, Router, Rule, Sink};

struct Uart;
impl Sink<u16> for Uart {
    fn send(&mut self, batch: &[u16]) -> Result<(), Busy> {
        // Write the batch to the serial port. Return Err(Busy) if the
        // transmit buffer is full.
        let _ = batch;
        Ok(())
    }
}

let mut uart = Uart;
let mut router: Router<u16, 1, 1, 8> = Router::new(
    [Lane::new(&mut uart, Policy::DropOldest)],
    [Rule { topic: None, lane: 0 }],
)
.unwrap();

// Filled from an interrupt handler:
let mut queue: Spsc<Envelope<u16>, 16> = Spsc::new();
let (mut tx, mut rx) = queue.split();
tx.push(Envelope { topic: 0, value: 512 }).unwrap();

// Drained from the main loop:
let routed = router.pump(&mut rx);
assert_eq!(routed.delivered, 1);
assert_eq!(router.flush(), 0); // no sink was busy
assert_eq!(router.stats(0).map(|s| s.sent), Some(1));
```

The type parameters of `Router<T, D, R, B>` are the message type, the number of lanes, the number of rules and the batch size.

## More examples

| Example | What it shows | Command |
|---|---|---|
| `throughput` | Host throughput of `route` called directly, and of `pump` through an `Spsc` queue, with two lanes and 20 million messages | `cargo run --release -p tpt-dev-sink --example throughput` |

The numbers come from the host machine, not from a microcontroller.

## Policies

| Policy | When the batch is full and the sink is busy | Keeps |
|---|---|---|
| `DropNewest` | the new message is discarded and counted | the oldest queued data |
| `DropOldest` | the oldest queued message is discarded and counted | the freshest data |
| `Reject` | `push` returns `Err(Busy)` | everything, and the caller decides |

## Design notes and guarantees

- **No allocation.** Lanes, the rule table and batches are fixed arrays. `Lane::new` fails to compile when `B` is zero.
- **Batch goes out when full.** A lane sends as soon as its batch reaches `B`. If the sink refuses, the batch stays full and the policy applies on the next `push`, which first retries the send.
- **Routing is bounded.** `route` checks each of the `R` rules once and does one lane push per match.
- **Matching.** Rules are checked in order and every match delivers a copy. `Routed::delivered` counts lane deliveries, so a message that matches two rules counts twice.
- **Only `Reject` reports errors.** With the drop policies, `Lane::push` always returns `Ok`.
- **`Lane` and `Router` require `T: Copy + Default`.**

## Limitations and scope

- **Not shared between threads.** A lane's batch is owned by one router, which is not meant to be shared. Share data with the `Spsc` ingress queue.
- **Lock-free applies to the ingress queue.** The queue is single-producer, single-consumer. The router and lanes do no locking.
- **Sinks are borrowed.** A `Lane` holds `&mut dyn Sink<T>`, so the sink cannot be used elsewhere while the router is alive.
- **Topics are `u16`.** Matching is exact or wildcard only.
- **A full batch is not split.** A lane sends its whole batch or none of it.
- **No retry timing.** A busy sink is retried on the next `push` or `flush`, not on a timer.

## Related crates

- [`tpt-dev-lock`](https://docs.rs/tpt-dev-lock): provides the `Spsc` queue used for ingress (dependency).
- [`tpt-dev-trace`](https://docs.rs/tpt-dev-trace): `TraceSink` adapts its `Sink` trait to a lane (dependency).

## Status and MSRV

Pre-release (0.0.0, not yet published). The minimum supported Rust version is 1.98 (`rust-version` in `Cargo.toml`).

## License

Licensed under either of MIT or Apache-2.0, at your option.

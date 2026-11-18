# tpt-dev-sink

Zero-allocation, lock-free edge telemetry router for microcontrollers.

- **Heap-free batching:** each destination (`Lane`) has a fixed array of `B` messages. It sends a whole batch at once.
- **Static routing table:** `Rule`s match a topic (or all topics) and name a lane. The table has fixed capacity and is checked when the router is built.
- **Explicit back-pressure:** when a sink is busy, each lane applies a `Policy`: drop the newest, drop the oldest, or reject and tell the caller.
- **Lock-free ingress:** a `tpt-dev-lock` `Spsc` queue carries messages from an interrupt handler to the router. The router drains it with `pump`.
- **Trace integration:** `TraceSink` lets `tpt-dev-trace` records use the same lanes.

## Example

```rust
use tpt_dev_sink::{Busy, Envelope, Lane, Policy, Router, Rule, Sink};
use tpt_dev_lock::Spsc;

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
router.pump(&mut rx);
router.flush();
```

## Policies

| Policy | When the batch is full and the sink is busy | Keeps |
|---|---|---|
| `DropNewest` | the new message is discarded and counted | the oldest queued data |
| `DropOldest` | the oldest queued message is discarded and counted | the freshest data |
| `Reject` | `push` returns `Err(Busy)` | everything, and the caller decides |

## Scope

- A lane's batch is owned by one router, so the router is not shared between threads. Share data with the `Spsc` ingress queue.
- Throughput on the host: `cargo run --release -p tpt-dev-sink --example throughput`.

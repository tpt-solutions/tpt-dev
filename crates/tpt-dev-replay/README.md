# tpt-dev-replay

Deterministic recording and replay for `tpt-dev-async` executors. A run that crashes can be replayed exactly, with the same schedule, interrupts and inputs.

- **Compact event log:** each event is 16 bytes. `Log<N>` is append-only and fills up, so it can always be replayed. `Ring<N>` overwrites the oldest events and counts what it lost.
- **Recorded decisions:** the driver logs each interrupt, each scheduling choice and each input that a task reads.
- **Deterministic replay:** a replay follows the log. If the program asks for something the log does not have, the replay reports a `Divergence` with the event index.
- **Trace integration:** `Event::to_trace_record` turns an event into a `tpt-dev-trace` record, so a run can be exported through the trace pipeline.

## Example

```rust,ignore
use core::cell::RefCell;
use tpt_dev_replay::{Log, Session, step};

// Record: the seed decides the interrupts and the inputs.
let session = RefCell::new(Session::<64>::recording(7));
// ... spawn tasks that call `session.borrow_mut().input(channel)`, then
// loop on `step(&session, exec.as_mut())` until it returns `Progress::Idle`.

// Replay: the same program, driven by the recorded log.
let script: Log<64> = *session.borrow().log();
let replay = RefCell::new(Session::<64>::replaying(script));
// ... the same loop now follows the log, and the replay's log equals `script`.
```

The crash test in `tests/crash.rs` runs the full loop. A recording crashes on some seed. The replay of its log then fails with the same message, after the same events, and leaves an identical log.

## Limits

- **Interrupts are simulated.** The driver wakes a task at a point it chooses, and the choice is logged. Real interrupt timing is not captured, so a crash that depends on interrupt timing inside a poll is not reproduced unless the driver fires it at the same step.
- **Inputs must go through the session.** A task that reads the clock or the hardware directly will not replay. Route every external value through `Session::input`.
- **Replay copies the log.** `Session::input` copies the script on each call, so keep `N` modest on small targets.
- **A full log cannot grow.** When a `Log` is full, recording stops with a `Divergence` (`room in the log`). Use a larger `N`, or a `Ring` for monitoring.
- **Tasks must not hold a session borrow across a poll.** The driver borrows the session too, so a held borrow causes a panic.
- **Host driver.** The driver runs on the host. On a device, the same log format can be written out and read back, but the driver is not wired to hardware.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

# tpt-dev-replay

Deterministic execution recorder for replaying concurrent and embedded crashes.

## Overview

`tpt-dev-replay` records the decisions that make a run of a [`tpt-dev-async`](https://docs.rs/tpt-dev-async) executor non-deterministic, and replays them. A recording session drives the executor itself: it picks which ready task to poll, fires simulated interrupts, and hands out input values from a seeded generator. Every one of those choices is appended to a log. A replay session reads the same choices back from that log, so a run that crashed can be reproduced with the same schedule, interrupts and inputs.

It is for debugging concurrency and interrupt bugs in code written against `tpt-dev-async`. Events are fixed 16-byte records, so a log can be written out and read back, and `Event::to_trace_record` converts events into [`tpt-dev-trace`](https://docs.rs/tpt-dev-trace) records.

## Features

- **Compact event log:** each `Event` encodes to 16 bytes. `Log<N>` is append-only and refuses new events when full, so a log is always replayable. `Ring<N>` overwrites the oldest events and counts what it lost.
- **Recorded decisions:** the driver logs each interrupt, each scheduling choice and each input a task reads.
- **Deterministic replay:** a replay follows the log. If the program asks for something the log does not have, the call returns a `Divergence` with the log index.
- **Seeded recording:** the seed fixes the inputs and the interrupt choices, so a recording is itself repeatable. `set_interrupt_permille` sets how often an interrupt fires.
- **Trace integration:** `Event::to_trace_record` turns an event into a `tpt-dev-trace` `Record`.

## Installation

```sh
cargo add tpt-dev-replay
```

The crate is `#![no_std]` and needs no allocator. It is not published yet; until then, depend on it by path or git.

| Feature | Enables | Default |
|---|---|---|
| `std` | links `std` (used for tests and host tooling) | off |

The CI builds the workspace for `thumbv7em-none-eabihf`.

## Quick start

```rust
use core::cell::{Cell, RefCell};
use core::pin::pin;
use tpt_dev_async::Executor;
use tpt_dev_replay::{Log, Progress, Session, step};

// A program with one task that reads one input through the session.
fn run(session: &RefCell<Session<16>>) -> u64 {
    let out = Cell::new(0);
    let mut exec = pin!(Executor::<1>::new());
    let task = pin!(async {
        out.set(session.borrow_mut().input(0).unwrap());
    });
    exec.as_mut().spawn(task, 1).unwrap();
    while let Ok(Progress::Polled { .. }) = step(session, exec.as_mut()) {}
    out.get()
}

// Record: the seed decides the inputs.
let recording = RefCell::new(Session::<16>::recording(7));
let first = run(&recording);
let script: Log<16> = *recording.borrow().log();

// Replay: the same program, driven by the recorded log.
let replay = RefCell::new(Session::<16>::replaying(script));
let second = run(&replay);

assert_eq!(first, second);
assert_eq!(replay.borrow().log(), &script);
```

The crash test in `tests/crash.rs` runs a larger scenario. Two producers write inputs to a shared cell and a watcher panics on one value. A recording crashes on some seed, and the replay of its log fails with the same message, after the same events, and leaves an identical log. A second test checks that a changed log is reported as a divergence.

## Design notes and guarantees

- **Event encoding.** A tag byte, a one-byte field (slot or channel), six unused bytes, then the 64-bit value in little-endian order. `Event::decode` returns `None` for an unknown tag.
- **Driver step.** `step` applies any interrupts, then polls one ready task. A recording picks the highest-priority ready slot, with the lowest index breaking ties. It uses `Executor::poll_slot`, `wake_slot`, `ready_mask` and `priority` from `tpt-dev-async`.
- **Replay checks.** A replay returns `Divergence { at, expected }` when the log has a different kind of event than the program asked for, when an input is on a different channel, when the log has no room, or when the scheduled slot is not ready.
- **Replay output.** A replay session logs the events it consumes, so a faithful replay ends with a log equal to the script.
- **Generator.** Inputs and interrupt choices come from xorshift64 seeded by the caller. A seed of zero is replaced by a fixed constant.
- **No allocation.** `Log`, `Ring` and `Session` are fixed-size and `Copy` or plain structs. `Log::new`, `Session::recording` and `Session::replaying` are `const fn`.

## Limitations and scope

- **Interrupts are simulated.** The driver wakes a task at a point it chooses, and the choice is logged. Real interrupt timing is not captured, so a crash that depends on interrupt timing inside a poll is not reproduced unless the driver fires it at the same step.
- **Inputs must go through the session.** A task that reads the clock or hardware directly will not replay. Route every external value through `Session::input`.
- **Replay copies the log.** `Session::input` copies the script on each call, so keep `N` modest on small targets.
- **A full log cannot grow.** When a `Log` is full, recording stops with a `Divergence` whose `expected` is `"room in the log"`. Use a larger `N`, or a `Ring` for monitoring.
- **`Ring` is not replayable in full.** A ring that has overwritten events lacks the start of the run. The session itself records into a `Log`.
- **Tasks must not hold a session borrow across a poll.** The driver borrows the session too, so a held borrow panics.
- **Host driver.** The driver runs on the host side of your program. On a device, the same log format can be written out and read back, but the driver is not wired to hardware.

## Related crates

- [`tpt-dev-async`](https://docs.rs/tpt-dev-async): the executor being driven (dependency).
- [`tpt-dev-trace`](https://docs.rs/tpt-dev-trace): `Event::to_trace_record` produces its `Record` type (dependency).

## Status and MSRV

Pre-release (0.0.0, not yet published). The minimum supported Rust version is 1.98 (`rust-version` in `Cargo.toml`).

## License

Licensed under either of MIT or Apache-2.0, at your option.

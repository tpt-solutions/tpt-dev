# tpt-dev-async

Hard real-time `no_std` async runtime with bounded execution times.

## Overview

`tpt-dev-async` is an async executor for firmware that must bound the work it does per scheduling round. It allocates nothing: tasks are futures that the caller owns (in a static or on the stack), and the executor keeps them in a fixed array of `N` slots. Each round polls every ready task at most once, so the scheduler's own work per round is at most `N` polls.

It is for embedded and real-time code that wants `async`/`await` without a heap. Delays are measured against a [`tpt_dev_trace::Clock`](https://docs.rs/tpt-dev-trace), so a hardware tick counter or a test clock can drive them. The executor also exposes slot-level primitives (`poll_slot`, `wake_slot`, `ready_mask`, `priority`) so that [`tpt-dev-replay`](https://docs.rs/tpt-dev-replay) can drive the schedule itself.

## Features

- **Static task storage:** the executor holds up to 32 tasks, each borrowed from memory the caller owns.
- **Bounded rounds:** a round polls each task that is ready when the round starts at most once. Tasks woken during a round run in the next one.
- **Priority scheduling:** within a round, the ready task with the highest priority value is polled first.
- **Interrupt-safe wake-ups:** a wake sets one bit with an atomic `fetch_or`. It takes no lock and does not allocate.
- **Hardware-tick delays:** `Delay` completes once a `Clock` reaches a deadline.
- **Work counters:** `Stats` reports rounds run, total polls, and the most polls in any one round.
- **Model-checked wake protocol:** the loom test in `tests/loom.rs` checks that a wake from another thread is not lost.

## Installation

```sh
cargo add tpt-dev-async
```

The crate is `#![no_std]` and needs no allocator. It is not published yet; until then, depend on it by path or git.

| Feature | Enables | Default |
|---|---|---|
| `std` | links `std` (used for tests and host tooling) | off |

The CI builds the workspace for `thumbv7em-none-eabihf`. The crate uses only 32-bit atomics.

## Quick start

```rust
use core::cell::Cell;
use core::pin::pin;
use tpt_dev_async::Executor;

let reading = Cell::new(0u32);
let mut exec = pin!(Executor::<4>::new());
let sensor = pin!(async { reading.set(42) });
exec.as_mut().spawn(sensor, 10).unwrap();

// One round polls the sensor task once. It finishes on that poll.
assert_eq!(exec.as_mut().run_round(), 1);
assert_eq!(exec.pending(), 0);
assert_eq!(reading.get(), 42);
```

Delays use any `Clock`. `ManualClock` is convenient in tests:

```rust
use core::pin::pin;
use tpt_dev_async::{Delay, Executor};
use tpt_dev_trace::{Clock, ManualClock};

let clock = ManualClock::new(0);
let mut exec = pin!(Executor::<2>::new());
let task = pin!(async { Delay::new(&clock, 3).await });
exec.as_mut().spawn(task, 1).unwrap();

exec.as_mut().run_round();
assert_eq!(exec.pending(), 1); // the clock is still at 0
clock.advance(3);
exec.as_mut().run_round();
assert_eq!(exec.pending(), 0);
assert!(clock.now() >= 3);
```

## Design notes and guarantees

- **No allocation.** Slots, wake records and the ready word are fields of the executor. `Executor::new` is a `const fn`, and it fails to compile unless `1 <= N <= 32`.
- **Bounded scheduler work.** `run_round` takes the ready set with one atomic swap, then polls each set bit once. It does at most `N` polls, so the scheduler adds a bounded amount of work per round. The time a task spends inside `poll` is the task's own responsibility.
- **Wake latency.** A task woken during a round waits for the next round. For an interrupt, the worst case is one full round plus the wake itself.
- **Wake is one atomic operation.** The waker's vtable functions only read the task's wake record and call `fetch_or` on the ready word. Cloning and dropping a waker do nothing else.
- **Priority ties.** When two ready tasks have the same priority, the lower slot index runs first.
- **Measuring.** `Executor::stats` counts rounds and polls. The counts are exact, but any timing you attach to them comes from the host. Measure worst-case execution time on the target or in a cycle-accurate simulator such as [`tpt-dev-sim`](https://docs.rs/tpt-dev-sim).
- **`unsafe`.** The crate opts out of the workspace `unsafe_code` denial for the waker vtable and for reading pinned task state. Each block has a `SAFETY` comment.

## Limitations and scope

- **The executor must stay pinned.** Once a task is spawned the executor must not move. Its methods take `Pin<&mut Self>`; use `core::pin::pin!` or a static.
- **At most 32 tasks.** The ready set is one `u32`.
- **`Delay` busy-polls.** A waiting `Delay` wakes its own task on every poll, so the executor keeps polling it. A design that sleeps until the next tick is not written.
- **No timer queue.** A task is not woken by a timer interrupt at its deadline. Delays complete by polling.
- **Wakers must not outlive the executor.** A waker kept after the executor is dropped points at freed memory. A waker kept after its task finished is safe, because the wake record lives in the executor.
- **No spawning from inside a task.** `spawn` needs `Pin<&mut Executor>`, which a running task does not have.
- **Finished slots are reused.** A slot freed by a finished task can be taken by a later `spawn`.

## Related crates

- [`tpt-dev-trace`](https://docs.rs/tpt-dev-trace): provides the `Clock` trait that `Delay` uses (dependency).
- [`tpt-dev-replay`](https://docs.rs/tpt-dev-replay): records and replays executor runs using the slot-level API (depends on this crate).

## Status and MSRV

Pre-release (0.0.0, not yet published). The minimum supported Rust version is 1.98 (`rust-version` in `Cargo.toml`).

## License

Licensed under either of MIT or Apache-2.0, at your option.

# tpt-dev-async

A `no_std` async executor for real-time work. It allocates nothing, and each round does a bounded amount of work.

- **Static task storage:** the executor holds up to 32 tasks, each borrowed from memory the caller owns, such as a static or a stack frame.
- **Bounded rounds:** a round polls each ready task at most once, so it does at most `N` polls. Tasks woken during a round run in the next one.
- **Priority scheduling:** in a round, higher-priority ready tasks are polled first.
- **Interrupt-safe wake-ups:** waking a task sets one bit with an atomic `fetch_or`, with no lock and no allocation. A waker can be called from an interrupt handler.
- **Hardware-tick delays:** `Delay` completes when a `tpt_dev_trace::Clock` reaches a deadline.
- **Model-checked wake protocol:** loom checks that a wake from another thread is never lost.

## Example

```rust
use core::pin::pin;
use tpt_dev_async::Executor;

let mut exec = pin!(Executor::<4>::new());
let sensor = pin!(async { /* read a sensor, then return */ });
exec.as_mut().spawn(sensor, 10).unwrap();
exec.as_mut().run_round();
```

## Worst-case behaviour

- A round does at most `N` polls. That bounds the scheduler's own work. The time a task spends in `poll` is the task's responsibility.
- A task woken during a round waits for the next round. The worst case for an interrupt is one full round plus the wake itself.
- `Stats` counts rounds, polls, and the largest round, so worst cases can be measured. The numbers come from the host, not from hardware. Measure WCET on the target or in a cycle-accurate simulator.

## Limits

- **Pinned executor.** Once a task is spawned, the executor must not move. Its methods take `Pin<&mut Self>` for that reason.
- **Up to 32 tasks.** The ready set is one `u32`.
- **`Delay` busy-waits.** A waiting `Delay` wakes its own task on every poll, so the executor keeps polling it. A design that sleeps until the next tick is not written yet.
- **No timer queue.** Waking a task at a deadline is done by polling, not by an interrupt.
- **Wakers must not outlive the executor.** A waker kept after the executor is dropped is a dangling pointer. A waker kept after its task finished is safe, because its wake record stays in the executor.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

# tpt-dev-lock

Deadlock-free synchronisation primitives with compile-time lock ordering.

## Overview

`tpt-dev-lock` provides the small set of shared-state primitives that the other `tpt-dev` runtime crates build on: a spin mutex whose lock order is checked by the type system, a wait-free single-writer register, and a lock-free single-producer single-consumer queue. None of them allocates, and the crate has no runtime dependencies.

The mutex gives every lock a level, written as a type (`L1`, `L2`, ...). A closure holding a lock at level `Lk` can only take locks at a strictly higher level, and the compiler rejects anything else. Two threads therefore cannot take the same pair of locks in opposite orders. The triple buffer and the queue are for passing data between exactly one writer and one reader, such as an interrupt handler and the main loop.

It is for `no_std` and embedded code, and it also works on the host. [`tpt-dev-trace`](https://docs.rs/tpt-dev-trace) uses the mutex for its collector, and [`tpt-dev-sink`](https://docs.rs/tpt-dev-sink) uses the queue as its ingress.

## Features

- **Lock levels in types:** each `Mutex` has a level. A nested lock must be at a strictly higher level, so lock orders that could deadlock do not compile. Re-locking at the same level is rejected the same way.
- **Wait-free register:** `TripleBuffer` is a single-writer, single-reader register. Neither side blocks or retries.
- **Lock-free queue:** `Spsc` is a fixed-capacity single-producer, single-consumer ring. `push` and `pop` take a bounded number of steps.
- **No heap:** storage is inline, in const-generic arrays.
- **Model-checked:** loom tests in `tests/loom.rs` cover the primitives (`RUSTFLAGS="--cfg loom"`).
- **Compile-fail tests:** `tests/compile_fail.rs` uses `trybuild` to check that wrong orders and same-level nesting are rejected.

## Installation

```sh
cargo add tpt-dev-lock
```

The crate is `#![no_std]` and needs no allocator. It is not published yet; until then, depend on it by path or git.

| Feature | Enables | Default |
|---|---|---|
| `std` | links `std` (used for tests and host tooling) | off |

The CI builds the workspace for `thumbv7em-none-eabihf`. The crate uses `AtomicBool`, `AtomicU8` and `AtomicUsize`, which that target has.

## Quick start

```rust
use tpt_dev_lock::{L1, L2, Mutex, Token};

let config: Mutex<L1, u32> = Mutex::new(1);
let stats: Mutex<L2, u32> = Mutex::new(0);

// Allowed: level 1, then level 2.
config.lock_with(Token::root(), |c, held| {
    stats.lock_with(held, |s, _| *s += *c);
});
stats.lock_with(Token::root(), |s, _| assert_eq!(*s, 1));
```

Taking `stats` and then `config` does not compile, because `L1` is not above `L2`:

```rust,compile_fail
use tpt_dev_lock::{L1, L2, Mutex, Token};

let config: Mutex<L1, u32> = Mutex::new(1);
let stats: Mutex<L2, u32> = Mutex::new(0);
stats.lock_with(Token::root(), |_, held| config.lock_with(held, |_, _| ()));
```

A single-writer register and a queue:

```rust
use tpt_dev_lock::{Spsc, TripleBuffer};

let mut latest = TripleBuffer::new(0u32, 0, 0);
let (mut writer, mut reader) = latest.split();
writer.write(42);
reader.with_latest(|v| assert_eq!(*v, 42));

let mut queue: Spsc<u8, 4> = Spsc::new();
let (mut tx, mut rx) = queue.split();
tx.push(1).unwrap();
assert_eq!(rx.pop(), Some(1));
```

## Design notes and guarantees

- **Levels.** `L1` to `L8` are aliases for `Succ<...<Nil>>`. `Above<Lower>` is implemented for the crate's own level types only. A `Token<L>` is zero-sized and `Copy`; `Token::root()` is the token for a thread that holds nothing.
- **Mutex.** `Mutex::lock_with` takes a token for the caller's current level and a closure. The lock is released when the closure returns, including when it unwinds. `Mutex::new` is a `const fn`, so a mutex can be a `static`. The lock spins with `core::hint::spin_loop`.
- **Triple buffer.** `write` and `with_latest` each do a fixed number of steps (one atomic swap, plus one load on the reader side), so both are wait-free. The reader sees the latest published value, never a torn one, and never an older value after a newer one. Values can be skipped. A value that is written and never read is overwritten, which drops it.
- **Spsc.** `N` must be a non-zero power of two, checked at compile time. `push` returns the value back when the queue is full. Items still queued when the `Spsc` is dropped are dropped.
- **Single producer, single consumer.** `split` takes `&mut self` and returns one `Producer`/`Consumer` pair (or one `Writer`/`Reader` pair), so the type system keeps it to one of each.
- **`unsafe`.** The crate opts out of the workspace `unsafe_code` denial to wrap `UnsafeCell`. Each block has a `SAFETY` comment.

## Limitations and scope

- **`Mutex` spins.** Use it for short critical sections only. It does not block or sleep.
- **Ordering covers this crate's locks only.** It does not prevent deadlock with locks from other code, or with an interrupt handler that waits on a lock the interrupted code holds.
- **Eight levels.** `L1` to `L8` are provided. Deeper nesting needs your own `Succ<...>` alias.
- **Tokens do not prove a lock is held.** They record the level of the enclosing `lock_with` closures, which is what limits the order. A `Token` is `Copy`, so code that copies one out of a closure can reuse it; keep tokens inside the closure that received them.
- **No poisoning, no timeout, no `try_lock`.**

## Related crates

- [`tpt-dev-trace`](https://docs.rs/tpt-dev-trace): depends on this crate for its collector mutex.
- [`tpt-dev-sink`](https://docs.rs/tpt-dev-sink): depends on this crate for its `Spsc` ingress queue.

## Status and MSRV

Pre-release (0.0.0, not yet published). The minimum supported Rust version is 1.98 (`rust-version` in `Cargo.toml`).

## License

Licensed under either of MIT or Apache-2.0, at your option.

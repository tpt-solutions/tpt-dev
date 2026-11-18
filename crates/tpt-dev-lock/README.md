# tpt-dev-lock

Deadlock-free synchronisation primitives with compile-time lock ordering.

- **Lock levels in types:** every `Mutex` has a level. A nested lock must be at a strictly higher level, so the compiler rejects lock orders that could deadlock.
- **Wait-free register:** `TripleBuffer` is a single-writer register. Neither side blocks or retries.
- **Lock-free queue:** `Spsc` is a fixed-capacity single-producer, single-consumer queue. It needs no heap.
- **Model-checked:** `loom` tests cover every primitive (`RUSTFLAGS="--cfg loom"`).
- **`no_std`:** no allocation and no dependencies for the runtime code.

## Lock ordering

```rust
use tpt_dev_lock::{L1, L2, Mutex, Token};

let config: Mutex<L1, u32> = Mutex::new(1);
let stats: Mutex<L2, u32> = Mutex::new(0);

// Allowed: level 1, then level 2.
config.lock_with(Token::root(), |c, held| {
    stats.lock_with(held, |s, _| *s += *c);
});
```

Taking `stats` and then `config` does not compile, because `L1` is not above `L2`.

## Single-writer register and queue

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

## Scope

- `Mutex` spins. Use it for short critical sections only.
- Lock ordering prevents deadlock between locks in this crate. It does not prevent deadlock with locks from other code, or with an ISR that waits on a lock held by the main context.

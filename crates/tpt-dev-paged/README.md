# tpt-dev-paged

no_std paged memory manager for LLM KV-caches on edge devices.

## Overview

An LLM decoder keeps a key-value (KV) cache that grows by one token at a time for each sequence. On a device with fixed memory, giving each sequence one contiguous buffer wastes space and fragments the pool. `tpt-dev-paged` splits a caller-provided buffer into equal pages, gives each sequence a table of pages, and shares pages between sequences where it can.

The caller supplies the storage and the limits (as const generics), and the pager never allocates. It is `no_std` and has no dependencies. It stands alone in the workspace, and it does not run a model. It only manages the bytes of the cache.

## Features

- **Fixed page pool:** the caller's buffer is split into `N` pages of `tokens_per_page * bytes_per_token` bytes. Every page is the same size, so there is no external fragmentation.
- **Per-sequence page tables:** each sequence has a table of at most `MAXP` pages, so it cannot grow without bound. There are `S` sequence slots.
- **Copy-on-write forks:** `fork` shares every page of a sequence, and a write to a shared page copies it first. This is how beam search shares a prompt.
- **LRU eviction:** when the pool is full, the least recently used other sequence is evicted whole. Its later accesses report `Error::Evicted`.
- **Metrics:** `stats` gives free pages, total pages, active sequences and evictions. `internal_fragmentation` gives the unused token slots in a sequence's last page.

## Installation

```sh
cargo add tpt-dev-paged
```

The crate is `#![no_std]` and does not need `alloc`.

| Feature | Enables | Default |
|---|---|---|
| `std` | Links `std`. It adds no public API. | off |

## Quick start

```rust
use tpt_dev_paged::Pager;

let mut storage = [0u8; 4 * 8];
// 4 pages, room for 2 sequences of at most 4 pages each. Each page holds
// 2 tokens of 4 bytes.
let mut pager: Pager<'_, 4, 2, 4> = Pager::new(&mut storage, 2, 4).unwrap();
let a = pager.open(1).unwrap();
pager.append(a, 3).unwrap();
pager.write(a, 0, &[1, 2, 3, 4]).unwrap();
let b = pager.fork(a, 2).unwrap();
pager.write(b, 0, &[9, 9, 9, 9]).unwrap(); // copies the shared page first
assert_eq!(pager.read(a, 0).unwrap(), &[1, 2, 3, 4]);
assert_eq!(pager.read(b, 0).unwrap(), &[9, 9, 9, 9]);
```

The type is `Pager<'a, N, S, MAXP>`: `N` pages, `S` sequence slots and `MAXP` pages per sequence. Sequences are named by the slot that `open` and `fork` return. The `id` argument is a label that `sequence_id` reads back.

## More examples

| Example | Shows | Command |
|---|---|---|
| `kv_workload` | A decode-style host workload with forks, closes and evictions. It prints page use, evictions, mean unused token slots and the operation rate. | `cargo run --release -p tpt-dev-paged --example kv_workload` |

## Design notes and guarantees

- **No heap.** All state is in fixed arrays sized by the const generics, and the data lives in the caller's buffer.
- **Reference counts per page.** A page returns to the pool when its count reaches zero. `close` releases only the pages no other sequence shares.
- **Copy-on-write.** `write` copies a page that more than one sequence holds, then updates the copy.
- **Eviction.** The victim is the least recently used active sequence other than the one being served, and it must hold pages. Eviction releases its pages, sets its length to zero and counts it in `Stats::evictions`. The evicted slot stays occupied until `close`.
- **Validated configuration.** `Pager::new` returns `Error::BadConfig` for zero sizes, more than 65,535 pages, an overflowing page size, or storage smaller than `N` pages.
- **Typed errors.** Failures are `Error` values: `BadConfig`, `OutOfPages`, `OutOfSlots`, `TooLong`, `NoSuchSequence`, `Evicted`, `BadToken` and `BadSize`.

## Limitations and scope

- **Eviction is all-or-nothing.** A whole sequence is evicted, so its prefix is lost too. The caller must recompute it.
- **No recovery of evicted data.** The pager does not save evicted pages anywhere. Reading them returns `Error::Evicted`.
- **Fixed limits.** The number of pages, sequences and pages per sequence are type parameters, and a page index is 16 bits.
- **Linear scans.** Finding a free page, a free slot or an eviction victim scans the arrays, so those operations take time proportional to `N` or `S`.
- **Single-threaded.** Every change needs `&mut self`, so the pager is not shared between threads.
- **Bytes only.** It stores raw bytes per token and has no notion of keys, values, layers or data types.
- **The benchmark is not a model of a device.** The host run measures the pager's bookkeeping, not memory bandwidth or a real model's decode time.

## Related crates

This crate has no dependencies on other `tpt-dev-*` crates, and no workspace crate depends on it. See the [tpt-dev workspace](https://github.com/tpt-solutions/tpt-dev) for the others.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

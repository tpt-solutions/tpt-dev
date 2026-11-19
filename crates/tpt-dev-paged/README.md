# tpt-dev-paged

A `no_std` page manager for LLM key-value caches on devices with fixed memory. The caller provides the storage. The pager uses no heap, and it shares pages between sequences where it can.

- **Fixed page pool:** the caller's buffer is split into `N` pages of `tokens_per_page × bytes_per_token`. There is no external fragmentation, because every page is the same size.
- **Per-sequence page tables:** each sequence has a table of at most `MAXP` pages, so it cannot grow without bound.
- **Copy-on-write forks:** `fork` shares every page, and a write to a shared page copies it first. This is how beam search shares a prompt.
- **LRU eviction:** when the pool is full, the least recently used other sequence is evicted whole. Its later accesses report `Evicted`.
- **Metrics:** `stats` gives free pages, active sequences and evictions. `internal_fragmentation` gives the unused token slots in a sequence's last page.
- **Host benchmark:** `examples/kv_workload.rs` runs a decode-style workload with forks, closes and evictions.

## Example

```rust
use tpt_dev_paged::Pager;

let mut storage = [0u8; 4 * 8];
// 4 pages, 2 tokens per page, 4 bytes per token, room for 2 sequences of 4 pages.
let mut pager: Pager<'_, 4, 2, 4> = Pager::new(&mut storage, 2, 4).unwrap();
let a = pager.open(1).unwrap();
pager.append(a, 3).unwrap();
pager.write(a, 0, &[1, 2, 3, 4]).unwrap();
let b = pager.fork(a, 2).unwrap();
pager.write(b, 0, &[9, 9, 9, 9]).unwrap(); // copies the shared page first
assert_eq!(pager.read(a, 0).unwrap(), &[1, 2, 3, 4]);
```

## Limits

- **Eviction is all-or-nothing.** A whole sequence is evicted, so its prefix is lost too. The caller must recompute it.
- **No recovery of evicted data.** The pager does not save evicted pages anywhere. Reading them returns `Evicted`.
- **Fixed limits.** The number of pages, sequences and pages per sequence are type parameters, and a page index is 16 bits.
- **Storage is the caller's.** The pager does not check that the storage is aligned for the data type.
- **Single-threaded.** The pager needs `&mut self` for every change, so it is not shared between threads.
- **Benchmark is not a model of a device.** The host run measures the pager's bookkeeping, not memory bandwidth or a real model's decode time.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

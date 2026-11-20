# Changelog

All notable changes to `tpt-dev-paged` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Pager`, a fixed-page KV-cache manager over caller-provided storage, with page, sequence and per-sequence limits set by const generics.
- Per-sequence page tables with `open`, `append`, `write`, `read` and `close`.
- Copy-on-write `fork`, so sequences share pages until one of them writes.
- Least-recently-used eviction of a whole sequence when the pool is full. Later accesses report `Error::Evicted`.
- `Stats` and `Pager::stats`, with free pages, total pages, active sequences and evictions, and `internal_fragmentation` for a sequence's last page.
- `sequence_id` and `tokens` accessors.
- An `Error` type for configuration, capacity, eviction and range failures.
- The `kv_workload` example, a decode-style host workload.
- The crate is `no_std` with no dependencies, and the pager never allocates.

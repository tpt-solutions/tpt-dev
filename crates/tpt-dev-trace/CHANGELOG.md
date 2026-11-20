# Changelog

All notable changes to `tpt-dev-trace` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Record` and `Kind`: fixed-size, `Copy` trace records for span starts, span ends and events.
- `Ring<N>`: a fixed-capacity ring buffer that overwrites the oldest record and counts what it dropped.
- `Collector<N>`: a ring behind a `tpt-dev-lock` mutex, with `record`, `len`, `dropped` and batched `flush`.
- `Sink` trait for exporting record batches.
- `Tracer` and `SpanGuard` for starting traces, creating child spans and recording events; a span's end and duration are recorded on drop.
- `Context`, `TraceId` and `SpanId`, with a fixed 16-byte little-endian encoding for propagation between devices.
- `IdSource` and `AtomicIds`, which build span IDs from a node number and a 32-bit atomic counter.
- `Clock` trait, implemented for `Fn() -> u64` closures, and `ManualClock` for targets with 64-bit atomics.
- Loom and multi-thread tests for concurrent recording.
- Optional `std` feature.

### Changed
- `loom` is now taken from the workspace dependency table.

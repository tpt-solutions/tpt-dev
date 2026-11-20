# Changelog

All notable changes to `tpt-dev-replay` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Event` (`Schedule`, `Interrupt`, `Input`) with a fixed 16-byte encoding and `decode`.
- `Log<N>`: an append-only event log that refuses new events when full.
- `Ring<N>`: a ring of the newest events that counts overwritten ones.
- `Session<N>`: records a run from a seed, or replays a recorded `Log`, with `input` for task-visible inputs and `set_interrupt_permille` for simulated interrupts.
- `step`: drives a `tpt-dev-async` `Executor` one step at a time, recording or replaying the schedule.
- `Divergence` and `Progress` result types.
- `Event::to_trace_record`, which converts events into `tpt-dev-trace` records.
- A crash test that records a concurrent failure, replays it bit for bit, and checks that a tampered log is reported as a divergence.
- Optional `std` feature.

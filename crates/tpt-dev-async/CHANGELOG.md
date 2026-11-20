# Changelog

All notable changes to `tpt-dev-async` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Executor<N>`: a fixed-capacity, heap-free async executor for up to 32 tasks, with priority ordering and at most `N` polls per round.
- Interrupt-safe wakers that set one bit in an atomic ready word.
- `Executor::run_round` and `run_until_idle` for driving the executor.
- `Stats` counters for rounds, polls and the largest round.
- `Delay`, a future that completes when a `tpt_dev_trace::Clock` reaches a deadline.
- Slot-level primitives `poll_slot`, `wake_slot`, `ready_mask`, `priority` and `pending`, for drivers such as replayers that choose the schedule.
- A loom model of the wake protocol (build with `--cfg loom`).
- Optional `std` feature.

### Changed
- `loom` is now taken from the workspace dependency table.

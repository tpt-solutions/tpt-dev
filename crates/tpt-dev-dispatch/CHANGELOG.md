# Changelog

All notable changes to `tpt-dev-dispatch` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Device` trait (`kind`, `supports`, `cost`, `run`), with `Kind` and `Op` (`Add`, `Mul`, `Relu`, `Sum`).
- `CpuDevice`, a reference device that supports every operation.
- `MockAccelerator`, a simulated device that supports a chosen set of operations and records its calls.
- `Graph` and `TaskId` for building task graphs from vector operations, with arity and reference checks and a topological order.
- `schedule`, an earliest-finish-time list scheduler that returns a `Schedule` of `Assignment`s and a makespan.
- `execute`, which runs a schedule on its devices and returns each task's result.
- `GraphError` and `ScheduleError` for invalid graphs, unsupported operations and refused tasks.
- Optional `std` feature.

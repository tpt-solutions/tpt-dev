# Changelog

All notable changes to `tpt-dev-sim` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Kernel<N>`: a fixed-capacity, single-clock signal kernel with two-phase cycles, so results do not depend on component order.
- `Component`, `Ctx` and `SignalId` for writing clocked components.
- `Failure` and `SimError`; the kernel counts failures, keeps the first, and reports two components driving one signal as a failure.
- `Deadline` and `MinGap` timing checks.
- `Vcd`, a Value Change Dump writer over any `core::fmt::Write`.
- `timer::Timer`, a periodic timer peripheral with `CTRL`, `PERIOD` and `COUNT` registers and an interrupt pulse.
- An integration test that programs the timer through typed `tpt-dev-mem` registers.
- Optional `std` feature.

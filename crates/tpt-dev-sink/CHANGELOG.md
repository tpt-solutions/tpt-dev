# Changelog

All notable changes to `tpt-dev-sink` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Lane`: a destination with a fixed batch buffer, a `Sink`, and a back-pressure `Policy` (`DropNewest`, `DropOldest`, `Reject`).
- `Sink` trait and `Busy` error for destinations that cannot take a batch.
- `LaneStats` counters for sent, dropped and pending messages.
- `Router`: a fixed table of `Rule`s that fans messages out to lanes by topic, with `route`, `pump` and `flush`.
- `Envelope` and `Router::pump` for draining a `tpt-dev-lock` `Spsc` ingress queue.
- `RuleError`, returned when a rule names a lane that does not exist.
- `TraceSink`, which lets `tpt-dev-trace` exporters use lanes.
- A `throughput` example for host benchmarking.
- Optional `std` feature.

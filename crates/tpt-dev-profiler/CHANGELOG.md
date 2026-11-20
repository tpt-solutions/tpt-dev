# Changelog

All notable changes to `tpt-dev-profiler` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Sampler<N, D>`: a lock-free, non-allocating sample buffer that can be filled from an interrupt hook, with drop counting.
- `Symbols`: program-counter to function lookup over a caller-supplied sorted table.
- `Report`: self and inclusive counts per function, a text table (`write_text`) and folded stacks (`write_folded`) for flamegraph tools.
- `[unknown]` attribution for program counters outside the symbol table.
- `overhead` example that measures the per-sample cost on the host.
- A concurrency test for recording from several threads.

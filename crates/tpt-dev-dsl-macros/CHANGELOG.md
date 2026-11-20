# Changelog

All notable changes to `tpt-dev-dsl-macros` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `dsl!` proc-macro that parses a PEG-notation grammar at compile time and expands to a `tpt_dev_dsl::Grammar` constant.
- Notation for literals, ASCII byte ranges, rule calls, sequences, ordered choice, `*`, `?` and grouping.
- Compile errors on the offending token for undefined and duplicate rules, empty blocks, bad ranges and unsupported `+`.
- Shares the grammar validation of `tpt-dev-dsl` by compiling the same source file.

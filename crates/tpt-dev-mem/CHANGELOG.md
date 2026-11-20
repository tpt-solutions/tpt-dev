# Changelog

All notable changes to `tpt-dev-mem` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Reg<W, A>`, a typed handle to one memory-mapped register, created with the `unsafe` constructor `Reg::from_addr`.
- Access-mode types `RO`, `WO`, `RW` and `W1C`, with the `Readable` and `Writable` marker traits. Writing a read-only register does not compile.
- The sealed `Word` trait for register widths `u8`, `u16`, `u32` and `u64`.
- `Field<OFFSET, WIDTH>`, a const-generic bit field with `mask`, `get` and `set`.
- `Reg::read`, `write`, `modify`, `read_field` and `modify_field`, and `clear_bits` for write-1-to-clear registers.
- The `register_block!` macro for declaring a peripheral's registers at fixed offsets.
- The crate is `no_std`, has no dependencies, and uses volatile access for every hardware read and write.

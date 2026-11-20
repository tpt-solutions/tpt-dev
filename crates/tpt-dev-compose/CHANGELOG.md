# Changelog

All notable changes to `tpt-dev-compose` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- A core WebAssembly module parser (`parse`, `Module`) with bounds-checked index spaces and typed `ParseError` values.
- `Registry` and `Plan`, a linker that resolves function, table, memory, global and tag imports against exports, type-checks each binding, and orders modules so providers come first. Cycles are reported.
- A component binary parser (`parse_component`, `Component`) that lists sections, embedded core modules and nested components up to a depth of 16.
- Decoding of component import, export, alias, type and canonical sections, including outer aliases checked against the scopes they name.
- Component value, resource, function, component, instance and core module types in the `types` module.
- Canonical ABI `layout`, `flatten`, `tuple_layout` and `func_abi`, including the spill to memory for large signatures, with bounded work (`TooComplex`).
- Linking of component imports to registered components by name and kind, with structural type comparison.
- WAT fixtures checked against `wasm-tools`, and tests for malformed input, including every prefix and single-byte corruption of the construct fixtures.
- A runtime (`runtime::Runtime`) that instantiates linked core modules in plan order, binds function and immutable global imports, runs start functions, and invokes exports. It runs an integer subset (`i32` and `i64`, control flow, calls, locals and globals) with fuel and call-depth limits. Memories, tables, floats and unsupported instructions are refused at instantiation.
- Stored function bodies and global initialisers on `Module` (`code`, `global_inits`), for the runtime.
- The crate is `no_std` with `alloc`, and has no dependencies. Components are planned, not instantiated.

# Changelog

All notable changes to `tpt-dev-ast` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Stream<DEPTH, CARRY>`, a streaming JSON-like parser that accepts chunks of any size in fixed memory.
- `Event` and `Visitor` (implemented for any `FnMut(Event)` closure) for zero-copy events.
- `parse_all` for a complete in-memory document.
- `ParseError` with byte offsets for lexing errors, syntax errors, trailing data, excess depth, over-long tokens and truncated input.
- `Parser`, `Stack`, `Token` and the `lex` function as public building blocks.
- `Lexer` trait, `Step` and `Feed<L, CARRY>`, a chunked driver for any grammar's lexer, with `JsonLexer` as the default.
- `no_std` without `alloc`, and an optional `std` feature.

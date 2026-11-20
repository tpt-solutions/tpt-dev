# Changelog

All notable changes to `tpt-dev-dsl` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Expr`, `Rule` and `Grammar` for PEG grammars written as constants, with compile-time check that every `Call` names an existing rule.
- `Grammar::parse` and `Grammar::recognise`, an interpreter that builds a parse tree or checks input without allocating.
- `ParseError` and `Expected` that report the furthest failure position and what was expected.
- `MAX_DEPTH` recursion limit, so left recursion gives an error instead of a stack overflow.
- Packrat matcher: `parse_packrat`, `recognise_packrat` and a fixed-size `Memo<N>`, with `MemoFull` instead of a wrong answer when the table fills.
- `PackratStream` and `GrammarLexer`, which match a grammar against chunked input through the `tpt-dev-ast` streaming driver. The matcher resumes as bytes arrive, and keeps only the bytes and memo entries it can still use.
- `dsl!` macro behind the host-only `macros` feature, through `tpt-dev-dsl-macros`.
- `no_std` with `alloc`, and an optional `std` feature.

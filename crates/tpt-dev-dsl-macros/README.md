# tpt-dev-dsl-macros

The `dsl!` macro for tpt-dev-dsl: parses a grammar at compile time, with error spans.

## Overview

This crate reads a grammar written in PEG notation, checks it, and expands to a `tpt_dev_dsl::Grammar` constant. Errors point at the offending token. You normally do not depend on it directly. Enable the `macros` feature of [`tpt-dev-dsl`](https://docs.rs/tpt-dev-dsl) and use `tpt_dev_dsl::dsl!`.

It is a proc-macro crate, so it runs on the host at compile time only and is not a `no_std` crate. It depends on `syn`, `quote` and `proc-macro2`.

## Features

- **PEG notation:** literals, byte ranges, rule calls, sequences, ordered choice, `*`, `?` and grouping.
- **Compile-time checks:** undefined and duplicate rule names, empty blocks and bad ranges are errors on the offending token.
- **Same validation as `Grammar::new`:** the grammar module of `tpt-dev-dsl` is compiled into this crate from that crate's source by path, so the two cannot drift.

## Installation

```sh
cargo add tpt-dev-dsl --features macros
```

There are no feature flags on this crate. It is host-only (`proc-macro = true`).

## Quick start

```rust
use tpt_dev_dsl::dsl;

dsl! {
    pub ARITH {
        expr = term (space ("+" / "-") space term)*;
        term = factor (space ("*" / "/") space factor)*;
        factor = number / "(" space expr space ")";
        number = ['0'-'9'] ['0'-'9']*;
        space = " "*;
    }
}

assert!(ARITH.recognise(0, b"1 + 2 * (3 - 4)"));
```

This crate sets `doctest = false`, because its examples need `tpt-dev-dsl`. This example is compiled and run as a doctest of `tpt-dev-dsl` (with `--features macros`). The macro is also tested in `tpt-dev-dsl`'s `tests/dsl_macro.rs`, with compile-fail cases in `tests/ui`.

## Notation

- `"text"` matches those bytes. `""` matches nothing.
- `['a'-'z']` matches one ASCII byte in the range.
- `name` calls a rule defined in the same block.
- `e1 e2` is a sequence, and `e1 / e2` is an ordered choice.
- `e*` is zero or more, and `e?` is optional.
- `( e )` groups.

Rules are numbered in the order written, and the first rule is the start rule.

## Design notes and guarantees

- Undefined rule names, duplicate rule names and an empty block are errors, reported on the name.
- Ranges must be ASCII and in order.
- `+` is rejected with a message. Write `e e*` instead.
- The finished grammar goes through the same validation as `Grammar::new`.

## Limitations and scope

- The macro expands to paths starting with `::tpt_dev_dsl`, so the crate must be depended on under that name.
- Only the expressions of `tpt_dev_dsl::Expr` are available. There are no actions, predicates (`&`, `!`), or named captures.
- Only the first error in a block is reported.

## Related crates

- [`tpt-dev-dsl`](https://docs.rs/tpt-dev-dsl): the runtime and the public entry point. It depends on this crate through its `macros` feature.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

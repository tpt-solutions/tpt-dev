# tpt-dev-dsl

Domain-specific language framework built on `tpt-dev-ast`.

## Overview

`tpt-dev-dsl` is a framework for small languages. A grammar is a `const` value made of parsing expressions (PEG). The crate matches byte input against it and either builds a parse tree or reports the furthest byte it reached and what it expected there.

It is for embedded and `no_std` code that needs to parse a small, fixed grammar without a code generator, and for host code that wants the same grammar in a readable notation through the `dsl!` macro. The crate is `#![no_std]` and needs `alloc` for parse trees. Streaming input is built on the lexer hook in [`tpt-dev-ast`](https://docs.rs/tpt-dev-ast), and the macro lives in [`tpt-dev-dsl-macros`](https://docs.rs/tpt-dev-dsl-macros).

## Features

- **Grammars as constants:** `Grammar::new` checks at compile time that every rule reference exists. The grammar lives in static memory.
- **Parse trees:** `parse` returns a tree with one node per matched rule, with byte spans.
- **Allocation-free recognition:** `recognise` checks input without building a tree.
- **Useful errors:** a failure reports the furthest byte the parser reached, and what it was looking for there, as an `Expected` value.
- **Bounded recursion:** rules nest at most `MAX_DEPTH` (128) levels deep, so left recursion gives an error rather than a stack overflow.
- **Packrat matching:** `parse_packrat` and `recognise_packrat` memoise each rule's result at each position, so backtracking does not repeat matching (except where the depth limit forces a recheck). They give the same results as `parse` and `recognise`. The memo table is a caller-chosen `Memo<N>`, so its size is fixed. If it fills, they return `MemoFull` and never a wrong answer.
- **Streaming input:** `PackratStream` accepts byte chunks of any size, through the `tpt-dev-ast` `Feed` driver and the `GrammarLexer`. It matches as the bytes arrive, keeping only the bytes and memo entries it can still use, so input much longer than its buffers can stream through. It gives the same results as `parse_packrat` on the whole input.
- **`dsl!` macro (feature `macros`):** writes a grammar in PEG notation, checked at compile time, with errors on the offending token.

## Installation

```sh
cargo add tpt-dev-dsl
```

| Feature  | Enables | Default |
| -------- | ------- | ------- |
| `std`    | Links `std`. The crate itself uses only `core` and `alloc`. | off |
| `macros` | The `dsl!` macro, through `tpt-dev-dsl-macros`. Host-only, because it pulls in a proc-macro crate. Leave it off for `no_std` targets. | off |

The crate is `#![no_std]` and requires `alloc`.

## Quick start

```rust
use tpt_dev_dsl::{Expr, Grammar, Rule};

const DIGIT: Expr = Expr::Range(b'0', b'9');
const NUMBER: Grammar = Grammar::new(&[Rule {
    name: "number",
    body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
}]);

assert!(NUMBER.recognise(0, b"2024"));
let tree = NUMBER.parse(0, b"2024").unwrap();
assert_eq!((tree.start, tree.end), (0, 4));
assert!(NUMBER.parse(0, b"20x4").is_err());
```

Rule 0 is the start rule by convention. The `rule` argument of `parse` and `recognise` is a rule index, and `Grammar::rule_index(name)` finds one by name.

### Expression forms

- `Lit(bytes)` and `Range(lo, hi)` match input bytes.
- `Seq`, `Choice`, `Repeat` (zero or more) and `Optional` combine expressions.
- `Call(index)` matches a rule by its index in the grammar.

### Errors

```rust
use tpt_dev_dsl::{Expected, Expr, Grammar, Rule};

const DIGIT: Expr = Expr::Range(b'0', b'9');
const NUMBER: Grammar = Grammar::new(&[Rule {
    name: "number",
    body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
}]);

let err = NUMBER.parse(0, b"20x4").unwrap_err();
assert_eq!(err.position, 2);
assert_eq!(err.expected, Expected::EndOfInput);
```

### Packrat matching

```rust
use tpt_dev_dsl::{Expr, Grammar, Memo, Rule};

const DIGIT: Expr = Expr::Range(b'0', b'9');
const NUMBER: Grammar = Grammar::new(&[Rule {
    name: "number",
    body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
}]);

let mut memo = Memo::<64>::new();
let tree = NUMBER.parse_packrat(0, b"2024", &mut memo).unwrap();
assert_eq!((tree.start, tree.end), (0, 4));

// Clear the memo before reusing it for another input.
memo.clear();
assert_eq!(NUMBER.recognise_packrat(0, b"7", &mut memo), Ok(true));
```

### Streaming

`PackratStream<TOKENS, MEMO>` matches a grammar against input that arrives in chunks. `TOKENS` is the ring buffer's size in bytes, and `MEMO` the number of memo slots.

```rust
use tpt_dev_dsl::{Expr, Grammar, PackratStream, Rule};

const DIGIT: Expr = Expr::Range(b'0', b'9');
const NUMBER: Grammar = Grammar::new(&[Rule {
    name: "number",
    body: Expr::Seq(&[DIGIT, Expr::Repeat(&DIGIT)]),
}]);

let mut stream: PackratStream<64, 16> = PackratStream::new(NUMBER, 0);
for chunk in [&b"20"[..], b"2", b"4"] {
    stream.feed(chunk).unwrap(); // any split of the input is fine
}
let tree = stream.finish().unwrap();
assert_eq!((tree.start, tree.end), (0, 4));
```

### The `dsl!` macro

With the `macros` feature, a grammar can be written in PEG notation. See [`tpt-dev-dsl-macros`](https://docs.rs/tpt-dev-dsl-macros) for the notation and checks.

```rust
# #[cfg(feature = "macros")]
# {
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
# }
```

The same arithmetic grammar, written with `Expr` values, is in the crate's `tests/arith.rs` as a reference.

## Design notes and guarantees

- **What is streamed:** each chunk goes through `tpt-dev-ast`'s `Feed` driver, which calls `GrammarLexer`. The lexer checks each byte against the grammar's alphabet (every byte named by a `Lit` or `Range`). A byte outside it fails at once with `StreamError::Invalid`, at its offset. A terminal is one byte, so the lexer holds no partial token and chunk boundaries are never visible to it.
- **What is kept, the bytes:** the stream holds a ring buffer of `TOKENS` bytes. The matcher reads a byte only at a position it can still decide, and a byte is dropped once no decision can read it again. A `Choice` keeps its start position while it tries an alternative, so a longer span needs more buffer, and `feed` returns `StreamError::TokensFull` with the offset and reads no further.
- **What is kept, the memo:** the memo table has `MEMO` slots. When it fills, entries below the lowest live position are dropped, since no call can start there again. If it is still full, the result is `StreamError::MemoFull`.
- **What is kept, the tree:** matching builds the parse tree as it goes, one node per matched rule. The tree's size grows with the input and is not bounded by `TOKENS`. `finish_recognise` returns a bool and keeps no tree.
- **Sticky errors:** after the first stream error, later calls return the same error.
- **Memo reuse:** memo entries are keyed by rule and position only, so call `Memo::clear` between inputs.
- **Dependencies:** only `tpt-dev-ast`, and the macro crate behind `macros`.

## Limitations and scope

- **Two matchers.** `parse` and `recognise` use a plain interpreter, which has no memo and can take exponential time on some grammars. Left recursion that branches is exponential for the interpreter. `parse_packrat` and `recognise_packrat` avoid that, at the cost of a memo table.
- **Streaming limits.** A decision can reach back only as far as `TOKENS` bytes: a `Choice` that is still trying an alternative keeps the input from its start, so a longer span stops with `TokensFull`. The memo keeps only entries at or after the lowest live position, and it stops with `MemoFull` when `MEMO` slots are not enough. The stream can therefore succeed where `parse_packrat` with the same memo size fails, but never with a different tree. The returned tree is still built in full, so its size grows with the input. Memo entries hold a copy of their subtree, so an entry's cost is its subtree's size, and a deeply nested grammar costs more memo than a flat one.
- **Error messages list one expectation.** The furthest failure is reported, with a single expected item, not every alternative that failed there.
- **No semantic actions.** The parser returns the tree. Users walk it to build their own values.
- **PEG semantics.** A `Choice` commits to its first success, and `Repeat` never backtracks into itself. Write grammars with that in mind.
- **Bytes only.** Terminals are single bytes, and ranges are byte ranges.

## Related crates

- [`tpt-dev-ast`](https://docs.rs/tpt-dev-ast): the streaming lexer driver used by `PackratStream`. Dependency.
- [`tpt-dev-dsl-macros`](https://docs.rs/tpt-dev-dsl-macros): the `dsl!` macro. Optional dependency, behind `macros`.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

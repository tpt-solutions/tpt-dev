# tpt-dev-ast

Zero-copy streaming AST parser framework that runs in a fixed stack buffer.

## Overview

`tpt-dev-ast` parses structured text as it arrives, in byte chunks of any size, using memory whose size is fixed at compile time. A lexer classifies bytes into tokens, a driver reassembles tokens that cross chunk boundaries in a fixed carry buffer, and a pushdown parser turns the tokens into events for a visitor. String and number payloads are slices of the input, so nothing is copied unless a token is split across chunks.

It is for embedded and `no_std` code that must handle input of any size without allocating, such as a JSON-like message arriving over a serial link. The bundled lexer and parser cover a JSON-like grammar. The chunked driver, the fixed-depth stack and the visitor are generic, so another grammar can supply its own `Lexer`. [`tpt-dev-dsl`](https://docs.rs/tpt-dev-dsl) does this to stream input to its grammar matcher.

## Features

- **Chunked input:** feed bytes in pieces of any size. Tokens split across chunks are reassembled in a fixed carry buffer.
- **Zero-copy events:** string and number payloads are slices of the input. Nothing is copied unless a token crosses a chunk boundary.
- **Bounded memory:** `Stream<DEPTH, CARRY>` uses fixed storage. Nesting depth and the longest token that can cross a chunk boundary are both const parameters.
- **No panics on bad input:** nesting that is too deep, bad bytes, and truncated input all return a `ParseError` with a byte offset.
- **Visitor API:** events go to any `FnMut(Event<'_>)` closure or a `Visitor` implementation.
- **Pluggable lexer:** `Feed` is the chunked driver for any `Lexer` implementation. `Stream` is the JSON instance of `Feed`.
- **No allocation, no dependencies.**

## Installation

```sh
cargo add tpt-dev-ast
```

| Feature | Enables | Default |
| ------- | ------- | ------- |
| `std`   | Links `std`. The crate itself uses only `core`, so this changes nothing in the API. | off |

The crate is `#![no_std]` and does not need `alloc`.

## Quick start

```rust
use tpt_dev_ast::{Event, Stream};

// Nesting up to 8 levels, and tokens up to 64 bytes can cross a chunk boundary.
let mut stream: Stream<8, 64> = Stream::new();
let mut names = 0;
let mut visit = |e: Event<'_>| {
    if let Event::String(name) = e {
        names += name.len();
    }
};

for chunk in [&br#"{"name": "te"#[..], &br#"mp", "id": 7}"#[..]] {
    stream.feed(chunk, &mut visit).unwrap();
}
stream.finish(&mut visit).unwrap();
assert_eq!(names, 4);
```

### A whole document in memory

`parse_all` feeds a complete input and finishes the stream.

```rust
use tpt_dev_ast::{Event, parse_all};

let mut keys = 0;
parse_all::<8, 32>(br#"{"a": [1, 2], "b": null}"#, &mut |e: Event<'_>| {
    if let Event::Key(_) = e {
        keys += 1;
    }
})
.unwrap();
assert_eq!(keys, 2);
```

### Errors

Every error carries a byte offset counted over the whole stream.

```rust
use tpt_dev_ast::{Event, ParseError, Stream};

let mut stream: Stream<2, 16> = Stream::new();
let mut ignore = |_: Event<'_>| {};
let err = stream.feed(b"[[[", &mut ignore).unwrap_err();
assert_eq!(err, ParseError::DepthExceeded { offset: 2, limit: 2 });
```

### A custom lexer

`Feed` drives any `Lexer`. The lexer classifies bytes and the sink consumes tokens. It must return `Step::Incomplete` rather than guess when a token could continue in the next chunk, unless `eof` is set.

```rust
use tpt_dev_ast::{Fault, Feed, Lexer, Step};

/// Runs of digits, as words.
struct Digits;

impl Lexer for Digits {
    type Token<'a> = &'a [u8];

    fn lex<'a>(&self, input: &'a [u8], eof: bool) -> Step<&'a [u8]> {
        let n = input.iter().take_while(|b| b.is_ascii_digit()).count();
        match input.first() {
            None => Step::Incomplete,
            Some(b' ') => Step::Skip(1),
            Some(_) if n == 0 => Step::Invalid { at: 0 },
            Some(_) if n == input.len() && !eof => Step::Incomplete,
            Some(_) => Step::Token { token: &input[..n], len: n },
        }
    }
}

let mut feed: Feed<Digits, 8> = Feed::new(Digits);
let mut count = 0;
let mut sink = |_: &[u8]| -> Result<(), Fault> {
    count += 1;
    Ok(())
};
feed.feed(b"12 3", &mut sink).unwrap();
feed.feed(b"45 6", &mut sink).unwrap();
feed.finish(&mut sink).unwrap();
assert_eq!(count, 3);
```

## Design notes and guarantees

- **Fixed memory:** a `Stream<DEPTH, CARRY>` stores a container stack of `DEPTH` entries and a carry buffer of `CARRY` bytes inline. It does not allocate, whatever the input size.
- **Offsets:** `ParseError` variants (`Lex`, `Syntax`, `TrailingData`, `DepthExceeded`, `TokenTooLong`, `UnexpectedEnd`) give the byte offset in the whole stream.
- **Token length limit:** a token longer than `CARRY` that is split across chunks fails with `TokenTooLong`.
- **Lexer contract:** a lexer must not look past `input` for its answer, and must report `Step::Incomplete` rather than guess when a token might continue.
- **Whole-input path:** `parse_all` still reads the input in pieces of the carry size, so the same memory bounds apply.

## Limitations and scope

- **JSON-like only in the bundle.** The lexer and structural parser cover objects, arrays, strings, numbers, `true`, `false` and `null`. Another grammar supplies its own `Lexer` and still needs its own state machine to consume the tokens.
- **Numbers are loosely checked.** Numbers are checked only for their character set, not for full number syntax.
- **Escapes are not decoded.** Escape sequences in strings are passed through as written, so the caller decodes them if it needs the text.
- **Events, not a tree.** The crate reports events to a visitor. It does not build a tree.

## Related crates

- [`tpt-dev-dsl`](https://docs.rs/tpt-dev-dsl): builds on `Feed` and `Lexer` to stream input to a grammar matcher.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

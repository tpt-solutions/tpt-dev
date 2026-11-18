# tpt-dev-ast

Zero-copy streaming parser for JSON-like documents. It runs in a fixed stack buffer, so it handles inputs of any size.

- **Chunked input:** feed bytes in pieces of any size. Tokens split across chunks are reassembled in a fixed carry buffer.
- **Zero-copy events:** string and number payloads are slices of the input. Nothing is copied unless a token crosses a chunk boundary.
- **Bounded memory:** `Stream<DEPTH, CARRY>` uses fixed storage. Nesting depth and the longest token that can cross a chunk boundary are both const parameters.
- **No panics on bad input:** nesting that is too deep, bad bytes, and truncated input all return a `ParseError` with a byte offset.
- **Visitor API:** events go to any `FnMut(Event<'_>)` closure or a `Visitor` implementation.
- **`no_std`:** no allocation and no dependencies.

## Example

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
```

## Scope

The lexer and the structural parser cover a JSON-like grammar: objects, arrays, strings, numbers, `true`, `false` and `null`. The framework parts are generic: the chunked driver (`Stream`), the fixed-depth stack, and the visitor. Other grammars would need their own lexer and state machine. Numbers are checked only for their character set, not for full number syntax. Escape sequences are not decoded, so the caller decodes them if it needs the text.

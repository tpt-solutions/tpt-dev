# tpt-dev-dsl

A framework for small languages. A grammar is a `const` value of parsing expressions (PEG). The crate matches input against it, and it builds a parse tree or reports where the input went wrong.

- **Grammars as constants:** `Grammar::new` checks at compile time that every rule reference exists. The grammar lives in static memory.
- **Parse trees:** `parse` returns a tree with one node per matched rule, with byte spans.
- **Allocation-free recognition:** `recognise` checks input without building a tree.
- **Useful errors:** a failure reports the furthest byte the parser reached, and what it was looking for there, as an `Expected` value.
- **Bounded recursion:** rules nest at most `MAX_DEPTH` levels deep, so left recursion gives an error rather than a stack overflow.
- **Packrat matching:** `parse_packrat` and `recognise_packrat` memoise each rule's result at each position, so backtracking does not repeat matching (except where the depth limit forces a recheck). They give the same results as `parse` and `recognise`. The memo table is a caller-chosen `Memo<N>`, so memory is fixed. If it fills, they return `MemoFull` and never a wrong answer.
- **Streaming input:** `PackratStream` accepts byte chunks of any size, through the `tpt-dev-ast` `Feed` driver and the `GrammarLexer`. It gives the same results as `parse_packrat` on the whole input. See below.
- **`dsl!` macro (feature `macros`):** writes a grammar in PEG notation, checked at compile time, with errors on the offending token. See [`tpt-dev-dsl-macros`](../tpt-dev-dsl-macros).

## Streaming

`PackratStream<TOKENS>` matches a grammar against input that arrives in chunks:

```rust,ignore
use tpt_dev_dsl::{Memo, PackratStream};

let mut stream: PackratStream<4096> = PackratStream::new(ARITH, EXPR);
for chunk in input_chunks {
    stream.feed(chunk)?;   // any split of the input is fine
}
let tree = stream.finish(&mut Memo::<8192>::new())?;
```

What is streamed and what is buffered:

- **Streamed.** Each chunk goes through `tpt-dev-ast`'s `Feed` driver, which calls `GrammarLexer`. The lexer checks each byte against the grammar's alphabet (every byte named by a `Lit` or `Range`). A byte outside the alphabet fails at once with `StreamError::Invalid`, at its offset. Chunk boundaries are never visible to the lexer, which holds no partial token: a terminal is one byte.
- **Buffered: the terminals.** A terminal is one byte, and the stream keeps each accepted byte in a fixed buffer of `TOKENS` bytes, held in the struct. Packrat backtracking can return to any earlier position, so the matcher cannot run until the input ends. When the buffer is full, `feed` returns `StreamError::TokensFull` with the offset, and it does not read further. The buffer is the only per-input storage the stream has, and it is bounded.
- **Buffered: the memo.** The memo table is the caller's `Memo<N>`, with a fixed size. If it fills, the result is `StreamError::MemoFull`, never a wrong answer.
- **Buffered: the tree.** `finish` builds the parse tree on the heap, with one node per matched rule. Its size grows with the input and it is not bounded by `TOKENS`. `finish_recognise` builds no tree.

So the streaming driver holds no input it has not matched yet, but the matcher still needs the whole accepted input (up to `TOKENS` bytes) and a memo of `N` entries. Inputs larger than `TOKENS` are rejected, not truncated.

Errors after the first one are sticky: later calls return the same error.

## Example

The grammar in `tests/arith.rs` is a reference for arithmetic over numbers, with `+ - * /` and parentheses.

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

## Expression forms

- `Lit(bytes)` and `Range(lo, hi)` match input bytes.
- `Seq`, `Choice`, `Repeat` (zero or more) and `Optional` combine expressions.
- `Call(index)` matches a rule by its index in the grammar.

## Limits

- **Two matchers.** `parse` and `recognise` use a plain interpreter, which has no memo and can take exponential time on some grammars. `parse_packrat` and `recognise_packrat` avoid that, at the cost of a memo table. Left recursion that branches is exponential for the interpreter, so use the packrat matcher for such grammars.
- **Packrat streaming still buffers.** `PackratStream` streams the bytes, but the matcher needs the whole accepted input, the memo and the tree in memory. See [Streaming](#streaming).
- **Error messages list one expectation.** The furthest failure is reported, with a single expected item, not every alternative that failed there.
- **No semantic actions.** The parser returns the tree. Users walk it to build their own values.
- **PEG semantics.** A `Choice` commits to its first success, and `Repeat` never backtracks into itself. Grammars need to be written with that in mind.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

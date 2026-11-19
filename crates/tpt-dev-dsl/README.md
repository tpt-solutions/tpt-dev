# tpt-dev-dsl

A framework for small languages. A grammar is a `const` value of parsing expressions (PEG). The crate matches input against it, and it builds a parse tree or reports where the input went wrong.

- **Grammars as constants:** `Grammar::new` checks at compile time that every rule reference exists. The grammar lives in static memory.
- **Parse trees:** `parse` returns a tree with one node per matched rule, with byte spans.
- **Allocation-free recognition:** `recognise` checks input without building a tree.
- **Useful errors:** a failure reports the furthest byte the parser reached, and what it was looking for there, as an `Expected` value.
- **Bounded recursion:** rules nest at most `MAX_DEPTH` levels deep, so left recursion gives an error rather than a stack overflow.
- **Packrat matching:** `parse_packrat` and `recognise_packrat` memoise each rule's result at each position, so backtracking does not repeat matching (except where the depth limit forces a recheck). They give the same results as `parse` and `recognise`. The memo table is a caller-chosen `Memo<N>`, so memory is fixed. If it fills, they return `MemoFull` and never a wrong answer.
- **`dsl!` macro (feature `macros`):** writes a grammar in PEG notation, checked at compile time, with errors on the offending token. See [`tpt-dev-dsl-macros`](../tpt-dev-dsl-macros).

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
- **Not built on `tpt-dev-ast`.** The `ast` crate is a JSON-only lexer and grammar, so it cannot drive arbitrary grammars. The DSL matches its own grammars.
- **Error messages list one expectation.** The furthest failure is reported, with a single expected item, not every alternative that failed there.
- **No semantic actions.** The parser returns the tree. Users walk it to build their own values.
- **PEG semantics.** A `Choice` commits to its first success, and `Repeat` never backtracks into itself. Grammars need to be written with that in mind.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

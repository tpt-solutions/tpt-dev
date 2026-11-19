# tpt-dev-dsl-macros

The `dsl!` macro for [`tpt-dev-dsl`](../tpt-dev-dsl). It reads a grammar written in PEG notation at compile time, checks it, and expands to a `tpt_dev_dsl::Grammar` constant. Errors point at the offending token.

This crate is a proc-macro, so it runs on the host only. It is not a `no_std` crate. Users reach it through `tpt_dev_dsl::dsl!`, with the `macros` feature of `tpt-dev-dsl` enabled.

## Example

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

## Notation

- `"text"` matches those bytes. `""` matches nothing.
- `['a'-'z']` matches one ASCII byte in the range.
- `name` calls a rule defined in the same block.
- `e1 e2` is a sequence, and `e1 / e2` is an ordered choice.
- `e*` is zero or more, and `e?` is optional.
- `( e )` groups.

Rules are numbered in the order written, and the first rule is the start rule.

## Checks

- Undefined rule names, duplicate rule names and an empty block are errors, reported on the name.
- Ranges must be ASCII and in order.
- `+` is rejected with a message. Write `e e*` instead.
- The finished grammar goes through the same validation as `Grammar::new`. The module is compiled into this crate from `tpt-dev-dsl`'s source, so the two cannot drift.

## Limits

- The macro expands to paths starting with `::tpt_dev_dsl`, so the crate must be depended on under that name.
- Only the expressions of `tpt_dev_dsl::Expr` are available. There are no actions, predicates (`&`, `!`), or named captures.
- Only the first error in a block is reported.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

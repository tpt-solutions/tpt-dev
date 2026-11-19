# tpt-dev-lint

Project-specific lint rules for Rust source. Rules are written in Rust as syntax-tree visitors, so no compiler plugin or nightly toolchain is needed.

- **Rule trait:** a rule has a name and a `check` method over a parsed file, and returns findings with line and column.
- **Built-in rules:** `no-unwrap` (warn) flags `.unwrap()` and `.expect(..)`. `no-todo-macros` (warn) flags `todo!` and `unimplemented!`. `safety-comment` (deny) requires a `SAFETY` comment within three lines above each `unsafe` block. `doc-example` (warn) flags a `pub` free function, or a `pub` method in an inherent `impl`, whose doc comment has no fenced code block (```` ``` ````). `pub(crate)`, `pub(super)` and other restricted visibilities are not checked, nor are trait impl methods.
- **Test code is skipped:** `#[test]` functions and `#[cfg(test)]` modules are not checked.
- **Config file:** one `rule = allow|warn|deny` line per rule. Unknown rule names are an error.
- **Clippy-style output:** `severity[rule]: message`, with the location.
- **Exit codes:** `tpt-dev-lint` exits 1 if any finding is at deny severity, and 2 on a usage or read error.

## Usage

```sh
cargo run -p tpt-dev-lint -- --config lint.toml src/lib.rs src/main.rs
```

A config file looks like this:

```text
# Allow unwrap in this project. Fail on a missing SAFETY comment.
no-unwrap = allow
safety-comment = deny
```

## Writing a rule

Implement the `Rule` trait, and walk the file with a `syn::visit::Visit` implementation. Return a `Finding` for each problem. Add the rule to a `Linter` with the severity it should default to.

## Limits

- **Syntactic only.** Rules see names and shapes, not types. `no-unwrap` flags any method called `unwrap`, even on a type that is not `Option` or `Result`.
- **Comments are read from the source text.** The `safety-comment` rule looks at the lines above a block, so a `SAFETY` comment further up does not count.
- **Test code is recognised by attributes.** A test helper that is not marked `#[test]` or `#[cfg(test)]` is checked.
- **Doc examples are read from literal doc text.** A doc attribute that is not a string literal, such as `#[doc = include_str!(..)]`, is not inspected, so it does not count as an example. Whether the enclosing type is public is not checked.
- **Macro-generated code is not expanded.** Code inside macro invocations other than `todo!` and `unimplemented!` is not analysed.
- **Host tool.** It uses `syn` and `std`, and CI excludes it from the bare-metal build.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

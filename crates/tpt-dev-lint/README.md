# tpt-dev-lint

Custom project-specific linting without compiler plugins.

## Overview

`tpt-dev-lint` checks Rust source files against project rules. Each rule walks a `syn` syntax tree and reports findings with a line and column, so no compiler plugin, `rustc` internals or nightly toolchain is involved. A config file sets each rule to `allow`, `warn` or `deny`.

It is for projects that want a few house rules, such as "no `.unwrap()`" or "every `unsafe` block needs a `SAFETY` comment", that Clippy does not cover in the form you want. It ships as a library and as a command-line tool of the same name. It is a host tool and does not target bare metal. It has no dependencies on other `tpt-dev-*` crates.

## Features

- **Rule trait:** a rule has a name and a `check` method over a parsed file, and returns findings with line and column.
- **Built-in rules:** `no-unwrap` (warn) flags `.unwrap()` and `.expect(..)`. `no-todo-macros` (warn) flags `todo!` and `unimplemented!`. `safety-comment` (deny) requires a `SAFETY` comment within three lines above each `unsafe` block, or on the block's own line. `doc-example` (warn) flags a `pub` free function, or a `pub` method in an inherent `impl`, whose doc comment has no fenced code block. `pub(crate)`, `pub(super)` and other restricted visibilities are not checked, nor are trait impl methods.
- **Test code is skipped:** `#[test]` functions and `#[cfg(test)]` modules are not checked.
- **Config file:** one `rule = allow|warn|deny` line per rule. Unknown rule names are an error.
- **Clippy-style output:** `severity[rule]: message`, with the location.
- **Exit codes:** the command exits 1 if any finding is at deny severity, and 2 on a usage, read or parse error.

## Installation

```sh
cargo add tpt-dev-lint
```

The command-line tool is built from the same package: `cargo run -p tpt-dev-lint -- <files>` inside the workspace.

| Feature | Enables | Default |
| --- | --- | --- |
| (none) | The crate has no feature flags. | n/a |

The crate is host-only: it uses `std`, `syn` and `proc-macro2`. CI excludes it from the `thumbv7em-none-eabihf` build.

## Quick start

```rust
use tpt_dev_lint::{Config, Linter, Severity, default_rules, has_errors};

let mut linter = Linter::new(default_rules());

// Make unwrap a hard error, and turn the doc-example rule off.
let config = Config::parse("no-unwrap = deny\ndoc-example = allow\n").unwrap();
linter.configure(&config).unwrap();

let source = "fn main() { let x: Option<u8> = None; x.unwrap(); }";
let diagnostics = linter.lint_source(source).unwrap();

assert_eq!(diagnostics.len(), 1);
assert_eq!(diagnostics[0].rule, "no-unwrap");
assert_eq!(diagnostics[0].severity, Severity::Deny);
assert!(has_errors(&diagnostics));
```

### Command line

```sh
cargo run -p tpt-dev-lint -- --config lint.cfg src/lib.rs src/main.rs
```

The tool takes `[--config FILE] <file.rs>...`. Each finding prints as `file:line:column severity[rule]: message`. A config file looks like this:

```text
# Allow unwrap in this project. Fail on a missing SAFETY comment.
no-unwrap = allow
safety-comment = deny
```

Blank lines and lines starting with `#` are ignored. The file is not TOML, so name it as you like.

## Writing a rule

Implement the `Rule` trait, and walk the file with a `syn::visit::Visit` implementation. Return a `Finding` for each problem. Add the rule to a `Linter` with the severity it should default to, by passing it in the `Vec` given to `Linter::new`.

## Design notes and guarantees

- **Deterministic output.** Diagnostics are sorted by line and column.
- **Rules at `allow` are skipped,** not run and filtered.
- **A bad config is rejected up front.** A malformed line, a bad severity or an unknown rule name is an error with a line number or the rule name, and the command exits 2.
- **Unparseable files are an error,** not a silent pass. `lint_source` returns `Err`, and the command exits 2.

## Limitations and scope

- **Syntactic only.** Rules see names and shapes, not types. `no-unwrap` flags any method called `unwrap`, even on a type that is not `Option` or `Result`.
- **Comments are read from the source text.** The `safety-comment` rule looks at the lines above a block, so a `SAFETY` comment further up does not count.
- **Test code is recognised by attributes.** A test helper that is not marked `#[test]` or `#[cfg(test)]` is checked.
- **Doc examples are read from literal doc text.** A doc attribute that is not a string literal, such as `#[doc = include_str!(..)]`, is not inspected, so it does not count as an example. Whether the enclosing type is public is not checked.
- **Macro-generated code is not expanded.** Code inside macro invocations other than `todo!` and `unimplemented!` is not analysed.
- **Host tool.** It uses `syn` and `std`, and CI excludes it from the bare-metal build.

## Related crates

`tpt-dev-lint` does not depend on, and is not depended on by, any other workspace crate. Other host tools in the workspace include [`tpt-dev-dep`](https://docs.rs/tpt-dev-dep).

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

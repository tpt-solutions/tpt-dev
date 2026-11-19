# tpt-dev-doc

Finds the code examples in documentation and renders them as static HTML. The examples come from Markdown, or from the doc comments of Rust source.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

- **Extraction:** `extract` finds the fenced code blocks and reads their tags as rustdoc does (`ignore`, `no_run`, `compile_fail`, `should_panic`, or untagged or `rust` for Rust). Lines starting with `# ` are hidden from the reader but kept in the compiled source.
- **Doc comments:** `doc_comments` pulls the `///` and `//!` text out of a Rust file, so its examples can be extracted.
- **Rendering:** `page` makes a complete HTML page, with every code block escaped. The page has no script and makes no network request.
- **Rustdoc injection:** `inject_before` inserts an HTML fragment before a marker in an existing rustdoc page, and reports when the marker is missing.

## Example

```rust
use tpt_dev_doc::{Mode, extract, page};

let md = "Adds two numbers.\n\n```rust\n# use std::ops::Add;\nassert_eq!(2 + 2, 4);\n```\n";
let examples = extract(md);
assert_eq!(examples.len(), 1);
assert_eq!(examples[0].mode, Mode::Run);
assert_eq!(examples[0].source, "use std::ops::Add;\nassert_eq!(2 + 2, 4);\n");
assert_eq!(examples[0].shown, "assert_eq!(2 + 2, 4);\n");

let html = page("Arithmetic", &examples);
assert!(html.contains("<h1>Arithmetic</h1>"));
```

## Not done yet

- **Running examples.** The design is to compile each example to Wasm and run it in the reader's browser, with no server. This crate does not do that yet. It needs the `wasm32-unknown-unknown` target, which is not installed in the development environment, and a browser runtime to test it. Until then, the examples are shown but not run.
- **Hosting.** The output is plain files. A docs.rs build can include them, but no build step writes them into a crate's documentation yet.

## Sandboxing review

- **Nothing here executes code.** Extracted examples are text. They are escaped before they go into HTML, and the page contains no script.
- **Checked:** escaping of `&`, `<`, `>`, quotes and apostrophes in example code and in the page title; the page has no `<script>` element and no network reference.
- **Planned runner:** a Wasm runner in the browser has the browser's own sandbox. The example runs on the reader's machine, so no server runs untrusted code. Before it ships, it needs limits on time and memory, and it must not insert example output into the page without escaping.

## Limits

- **Markdown subset.** Only fenced blocks are read. Indented code blocks and reference links are ignored.
- **Hidden lines.** A line that is only `#` is kept as an empty line in the compiled source. A line starting with `##` has one `#` removed, as in rustdoc.
- **No compilation here.** Extraction does not check that examples compile. That is left to `cargo test` on the crate that owns the docs.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

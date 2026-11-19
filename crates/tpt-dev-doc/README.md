# tpt-dev-doc

Finds the code examples in documentation and renders them as static HTML. The examples come from Markdown, or from the doc comments of Rust source.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

- **Extraction:** `extract` finds the fenced code blocks and reads their tags as rustdoc does (`ignore`, `no_run`, `compile_fail`, `should_panic`, or untagged or `rust` for Rust). Lines starting with `# ` are hidden from the reader but kept in the compiled source.
- **Doc comments:** `doc_comments` pulls the `///` and `//!` text out of a Rust file, so its examples can be extracted.
- **Rendering:** `page` makes a complete HTML page, with every code block escaped. The page has no script and makes no network request.
- **Rustdoc injection:** `inject_before` inserts an HTML fragment before a marker in a page, and reports when the marker is missing. `site::inject_into_rustdoc` applies it to every HTML page under a crate's `target/doc` directory. It needs `std`, and it must run after `cargo doc`, not from `build.rs`: rustdoc overwrites what a build script writes there. Wiring it in is a build step the user adds.
- **Hosting:** `site::write_examples_page` writes the static page next to rustdoc output. The page has no script, so it still works where docs.rs disables scripts. A test checks it for script, frame, form, image, link and event-handler elements, and for any URL.
- **Wasm:** `wasm::wasm_source` and `wasm::compile` build a `Mode::Run` example into a Wasm module with `rustc`. `wasm::RUNNER_JS` is a browser module that runs such a module in a Web Worker with a time limit. No page uses it yet.

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

- **Live REPL.** The runner exists, but no page connects it to the examples, and there is no input box. The Worker time limit has not been run in a browser. The Wasm module was run under Node 22: a passing example returned, and a failing `assert_eq!` trapped.
- **Build step for rustdoc.** Nothing runs `site::inject_into_rustdoc` after `cargo doc`. Add it to an `xtask` or a small binary.
- **Wasm limits.** Modules are built with `std`, so they are about 1.6 MB. Examples that define `fn main`, use inner attributes (`#![...]`), or need other crates are not built.

## Sandboxing review

Repeated for the Wasm runner. The review covers the compile step, which runs on the maintainer's machine, and the runner, which runs in the reader's browser.

- **The compile step runs `rustc` on example source, so use it on trusted documentation only.** `include_str!`, `include_bytes!` and `env!` read the compiling machine's files and environment, and the result is embedded in the Wasm that readers download. The compiler flags are fixed by the crate.
- **Runner: browser sandbox.** The Wasm module imports nothing. It cannot reach the DOM, the network or the file system. It runs in a Web Worker, which the page can terminate.
- **Runner: limits.** The default time limit is 5 s, enforced by the page with `worker.terminate()`. This is not yet tested in a browser. Memory is capped at 16 MiB with the linker flag `--max-memory`. The flag was checked by building, not by running out of memory.
- **Runner: output.** The page must show `detail` with `textContent`. `RUNNER_JS` has no `innerHTML`, `eval` or `importScripts`, and a test checks this.
- **Runner: page policy.** The page needs a Content-Security-Policy that allows `blob:` workers. The runner fetches only the URL it is given.
- **Static page.** `page` escapes all example text, and it has no script, so none of the above applies to it.

## Limits

- **Markdown subset.** Only fenced blocks are read. Indented code blocks and reference links are ignored.
- **Hidden lines.** A line that is only `#` is kept as an empty line in the compiled source. A line starting with `##` has one `#` removed, as in rustdoc.
- **No compilation here.** Extraction does not check that examples compile. That is left to `cargo test` on the crate that owns the docs.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

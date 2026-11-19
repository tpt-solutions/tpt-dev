# tpt-dev-doc

Finds the code examples in documentation and renders them as static HTML. The examples come from Markdown, or from the doc comments of Rust source.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

- **Extraction:** `extract` finds the fenced code blocks and reads their tags as rustdoc does (`ignore`, `no_run`, `compile_fail`, `should_panic`, or untagged or `rust` for Rust). Lines starting with `# ` are hidden from the reader but kept in the compiled source.
- **Doc comments:** `doc_comments` pulls the `///` and `//!` text out of a Rust file, so its examples can be extracted.
- **Rendering:** `page` makes a complete HTML page, with every code block escaped. The page has no script and makes no network request.
- **Rustdoc injection:** `inject_before` inserts an HTML fragment before a marker in a page, and reports when the marker is missing. `site::inject_into_rustdoc` applies it to every HTML page under a crate's `target/doc` directory. It needs `std`, and it must run after `cargo doc`, not from `build.rs`: rustdoc overwrites what a build script writes there. The post-`cargo doc` step below calls it.
- **Post-`cargo doc` step.** `site::run_post_doc_step` does two things. It injects a static, collapsed `<details>` examples block before `</body>` on every rustdoc page, and it writes the REPL page. It is the binary `tpt-dev-doc-site`, which needs the `std` feature.
- **Hosting:** `site::write_examples_page` writes the static page next to rustdoc output. The page has no script, so it still works where docs.rs disables scripts. A test checks it for script, frame, form, image, link and event-handler elements, and for any URL.
- **Wasm:** `wasm::wasm_source` and `wasm::compile` build a `Mode::Run` example into a Wasm module with `rustc`. `wasm::RUNNER_JS` is a browser module that runs such a module in a Web Worker with a time limit. `site::write_repl_page` compiles the runnable examples and writes the REPL page, which embeds `RUNNER_JS` inline.

## Running the post-`cargo doc` step

Run this after `cargo doc`. The doc directory is `target/doc/<crate_name>`, with hyphens in the crate name written as underscores. Pass the file whose doc comments hold the examples, or a Markdown file such as the crate README:

```sh
cargo doc -p my-crate --no-deps
cargo run -p tpt-dev-doc --features std --bin tpt-dev-doc-site -- target/doc/my_crate README.md "My crate"
```

The step writes `target/doc/my_crate/examples-repl.html` and `target/doc/my_crate/examples-repl-wasm/example-N.wasm`. Serve the directory over HTTP, such as `python -m http.server` from `target/doc`, and open the REPL page: browsers block `fetch` of `file://` URLs. A second run is safe. A failing example compile stops the step and names its line, and no REPL page is written.

The static examples block goes on every rustdoc page, so the examples repeat per page. The REPL page is separate and opt-in: it is not linked from rustdoc, and it is the only page with a script.

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

- **Live REPL in a browser.** The REPL page is generated and checked as text, and its Wasm modules run under Node 22 in a test: a passing example returned, a failing `assert_eq!` trapped, and no module imports anything. The page itself, the Worker time limit, and the Content-Security-Policy have not been run in a browser.
- **Wasm limits.** Modules are built with `std`, so they are about 1.6 MB. Examples that define `fn main`, use inner attributes (`#![...]`), or need other crates are not built.

## Sandboxing review

Repeated for the Wasm runner. The review covers the compile step, which runs on the maintainer's machine, and the runner, which runs in the reader's browser.

- **The compile step reads source on the build machine, so use it on trusted documentation only.** `rustc` compiles each example's source on the maintainer's machine. `include_str!`, `include_bytes!` and `env!` read that machine's files and environment, and the result is embedded in the Wasm that readers download. The compiler flags are fixed by the crate. `rustc` is run with `RUSTC` or `rustc`, and nothing is sandboxed.
- **Runner: browser sandbox.** The Wasm module imports nothing; a test checks this under Node. It cannot reach the DOM, the network or the file system. It runs in a Web Worker, which the page can terminate.
- **Runner: time limit.** The limit is 5 s, enforced by the page with `worker.terminate()`. It is **not verified in a browser**. The Node test does not cover it either, since Node has no Worker timeout path in this test. Treat it as unproven until it is run in a browser.
- **Runner: memory.** Memory is capped at 16 MiB with the linker flag `--max-memory`. The flag was checked by building, not by running out of memory.
- **Runner: output.** The page must show `detail` with `textContent`. `RUNNER_JS` has no `innerHTML`, `eval` or `importScripts`, and a test checks this. The REPL page's own script does not use `innerHTML` either.
- **REPL page: Content-Security-Policy.** The page carries its own CSP meta tag: `default-src 'none'`, inline script, `wasm-unsafe-eval`, `worker-src blob:`, and `connect-src 'self'`. **This CSP is not verified in a browser.** It is not known that a browser accepts blob Workers and WebAssembly under it, or that a meta CSP applies to the page as written. A server-sent header would be the stronger way to set it; the page does not depend on one.
- **REPL page: references.** The page refers only to its inline runner and to `examples-repl-wasm/example-N.wasm` by relative path. A test checks that it contains no URL and no other script, link, image or frame. Those wasm files are fetched by the page, which the CSP allows from its own origin.
- **Static page and static block.** `page` escapes all example text and has no script, so none of the above applies to it. The examples block injected into rustdoc pages is the same escaped markup, with no script.

## Limits

- **Markdown subset.** Only fenced blocks are read. Indented code blocks and reference links are ignored.
- **Hidden lines.** A line that is only `#` is kept as an empty line in the compiled source. A line starting with `##` has one `#` removed, as in rustdoc.
- **No compilation here.** Extraction does not check that examples compile. That is left to `cargo test` on the crate that owns the docs.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

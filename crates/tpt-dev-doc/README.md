# tpt-dev-doc

Interactive, runnable documentation with examples compiled to Wasm.

## Overview

Rustdoc shows code examples as text. `tpt-dev-doc` finds the examples in Markdown or in the doc comments of Rust source, renders them as static HTML, and can compile the runnable ones to WebAssembly so a reader can run them in the browser.

It has two layers. The core (`extract`, `doc_comments`, `page`, `inject_before`) is `no_std` with `alloc`. It finds fenced code blocks, reads their tags as rustdoc does, and renders escaped HTML with no script and no network request. The `std` layer (`site` and `wasm`) writes files, injects a block into rustdoc output after `cargo doc`, and builds the opt-in REPL page with `rustc`. It also provides the `tpt-dev-doc-site` binary.

The crate does not depend on other `tpt-dev-*` crates, and no workspace crate depends on it.

## Features

- **Extraction:** `extract` finds the fenced code blocks and reads their tags as rustdoc does (`ignore`, `no_run`, `compile_fail`, `should_panic`, or untagged or `rust` for Rust). Lines starting with `# ` are hidden from the reader but kept in the compiled source.
- **Doc comments:** `doc_comments` pulls the `///` and `//!` text out of a Rust file, so its examples can be extracted.
- **Rendering:** `page` makes a complete HTML page, with every code block escaped. The page has no script and makes no network request.
- **Rustdoc injection:** `inject_before` inserts an HTML fragment before a marker in a page, and reports when the marker is missing. `site::inject_into_rustdoc` applies it to every HTML page under a crate's `target/doc` directory. It needs `std`, and it must run after `cargo doc`, not from `build.rs`, because rustdoc overwrites what a build script writes there.
- **Post-`cargo doc` step:** `site::run_post_doc_step` injects a static, collapsed `<details>` examples block before `</body>` on every rustdoc page, and writes the REPL page. It is the binary `tpt-dev-doc-site`, which needs the `std` feature.
- **Hosting:** `site::write_examples_page` writes the static page next to rustdoc output. The page has no script, so it still works where docs.rs disables scripts. A test checks it for script, frame, form, image, link and event-handler elements, and for any URL.
- **Wasm:** `wasm::wasm_source` and `wasm::compile` build a `Mode::Run` example into a Wasm module with `rustc`. `wasm::RUNNER_JS` is a browser module that runs such a module in a Web Worker with a time limit. `site::write_repl_page` compiles the runnable examples and writes the REPL page, which embeds `RUNNER_JS` inline.

## Installation

```sh
cargo add tpt-dev-doc
```

To use the post-`cargo doc` binary, add the `std` feature.

| Feature | Enables | Default |
|---|---|---|
| `std` | The `site` and `wasm` modules (file output, rustdoc injection, the `rustc` compile step) and the `tpt-dev-doc-site` binary. | off |

Without `std`, the crate is `#![no_std]` and needs `alloc`. It has no dependencies.

## Quick start

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

## Running the post-`cargo doc` step

Run this after `cargo doc`. The doc directory is `target/doc/<crate_name>`, with hyphens in the crate name written as underscores. Pass the file whose doc comments hold the examples, or a Markdown file such as the crate README:

```sh
cargo doc -p my-crate --no-deps
cargo run -p tpt-dev-doc --features std --bin tpt-dev-doc-site -- target/doc/my_crate README.md "My crate"
```

The step writes `target/doc/my_crate/examples-repl.html` and `target/doc/my_crate/examples-repl-wasm/example-N.wasm`. Serve the directory over HTTP, such as `python -m http.server` from `target/doc`, and open the REPL page: browsers block `fetch` of `file://` URLs. A second run is safe. A failing example compile stops the step and names its line, and no REPL page is written.

The static examples block goes on every rustdoc page, so the examples repeat per page. The REPL page is separate and opt-in: it is not linked from rustdoc, and it is the only page with a script.

## Browser test

`tests/browser.rs` loads the generated REPL page in a headless browser. It covers three examples (a pass, a failing `assert_eq!`, and an infinite loop) and checks that the page's Content-Security-Policy blocks a cross-origin `fetch` and blocks nothing the page needs. The test is opt-in and needs `std`:

```sh
TPT_DEV_DOC_BROWSER=1 cargo test -p tpt-dev-doc --features std --test browser -- --nocapture
```

`TPT_DEV_DOC_BROWSER=1` uses Chrome or Edge from the default install paths. A path to another browser binary also works. Unset, set to a missing file, or with no browser found, the test prints why it skipped and passes. It starts the browser with a remote-debugging port, serves the page over HTTP on 127.0.0.1, and drives it with the DevTools protocol.

**Last run:** Google Chrome 157.0.8103.79 and Microsoft Edge 157.0.4341.60, both headless, on Windows 11 Pro, on the maintainer's machine. Both passed. The page showed:

- passing example: `passed` (about 21 ms)
- failing `assert_eq!`: `failed: unreachable` (about 21 ms; the Wasm trap)
- infinite loop: `stopped: time limit reached` (about 5.05 s)
- CSP: zero violations for the examples, and one violation for the control `fetch`

**Not tested:** Firefox and Safari. Other versions of Chrome and Edge, and other operating systems, are also untested.

## Design notes and guarantees

- **Static output has no script.** `page` escapes all example text and has no script. The examples block injected into rustdoc pages is the same escaped markup, with no script.
- **Extraction follows rustdoc's tags.** An untagged fence or one tagged `rust` is Rust. Other tags map to `Mode::NoRun`, `CompileFail`, `ShouldPanic`, `Ignore` or `Text`. Each `Example` records its fence line, the compiled `source` (hidden lines included, `# ` removed) and the `shown` text (hidden lines removed).
- **Hidden lines.** A line that is only `#` is kept as an empty line in the compiled source. A line starting with `##` has one `#` removed, as in rustdoc.
- **Idempotent injection.** A second run of the post-`cargo doc` step does not inject twice.

## Sandboxing review

Not independently audited. The review below covers the compile step, which runs on the maintainer's machine, and the runner, which runs in the reader's browser.

- **The compile step reads source on the build machine, so use it on trusted documentation only.** `rustc` compiles each example's source on the maintainer's machine. `include_str!`, `include_bytes!` and `env!` read that machine's files and environment, and the result is embedded in the Wasm that readers download. The compiler flags are fixed by the crate. `rustc` is run with `RUSTC` or `rustc`, and nothing is sandboxed.
- **Runner: browser sandbox.** The Wasm module imports nothing; a test checks this under Node. It cannot reach the DOM, the network or the file system. It runs in a Web Worker, which the page can terminate.
- **Runner: time limit.** The limit is 5 s, enforced by the page with `worker.terminate()`. It is browser-tested in headless Chrome 157 and Edge 157 on one machine only: a looping example was stopped at about 5.05 s and reported `stopped: time limit reached`. Firefox and Safari are not tested. The Node test does not cover the timeout.
- **Runner: memory.** Memory is capped at 16 MiB with the linker flag `--max-memory`. The flag was checked by building, not by running out of memory.
- **Runner: output.** The page must show `detail` with `textContent`. `RUNNER_JS` has no `innerHTML`, `eval` or `importScripts`, and a test checks this. The REPL page's own script does not use `innerHTML` either.
- **REPL page: Content-Security-Policy.** The page carries its own CSP meta tag: `default-src 'none'`, inline script, `wasm-unsafe-eval`, `worker-src blob:`, and `connect-src 'self'`. In headless Chrome and Edge the inline runner, WebAssembly and the blob Worker all ran with zero CSP violations, and a cross-origin `fetch` from the same page was blocked by `connect-src`. Firefox and Safari are not tested. A server-sent header would be the stronger way to set it; the page does not depend on one.
- **REPL page: references.** The page refers only to its inline runner and to `examples-repl-wasm/example-N.wasm` by relative path. A test checks that it contains no URL and no other script, link, image or frame.

## Limitations and scope

- **Markdown subset.** Only fenced blocks are read. Indented code blocks and reference links are ignored.
- **No compilation in extraction.** Extraction does not check that examples compile. That is left to `cargo test` on the crate that owns the docs.
- **Other browsers.** The REPL page is browser-tested only in headless Chrome and Edge on one Windows machine. Firefox and Safari have not been run, so their Worker time limit and CSP handling are unknown.
- **Wasm limits.** Modules are built with `std`, so they are about 1.6 MB. Only `Mode::Run` examples are built. Examples that define `fn main`, use inner attributes (`#![...]`), or need crates other than `std` and `core` are not built.
- **Trusted input only for the compile step.** See the sandboxing review.

## Related crates

This crate has no dependencies on other `tpt-dev-*` crates, and no workspace crate depends on it. See the [tpt-dev workspace](https://github.com/tpt-solutions/tpt-dev) for the others.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

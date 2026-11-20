# Changelog

All notable changes to `tpt-dev-doc` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `extract` and `Example`, which find fenced code blocks in Markdown and read their rustdoc-style tags into a `Mode`. Hidden `# ` lines are kept in the compiled source and removed from the shown text.
- `doc_comments`, which pulls `///` and `//!` text out of a Rust file.
- `page`, `example_section` and `escape`, which render examples as static, escaped HTML with no script and no network request.
- `inject_before`, and `site::inject_into_rustdoc`, which insert an HTML fragment into every rustdoc page under a doc directory.
- `site::write_examples_page` for a static examples page that works where docs.rs disables scripts.
- `site::run_post_doc_step` and the `tpt-dev-doc-site` binary, a post-`cargo doc` step that injects a static examples block into every rustdoc page and writes the REPL page.
- `wasm::wasm_source` and `wasm::compile`, which build runnable examples to Wasm with `rustc` under a 16 MiB memory cap.
- `wasm::RUNNER_JS` and `site::write_repl_page`, an opt-in REPL page that runs each module in a Web Worker with a 5 second limit and a Content-Security-Policy.
- An opt-in test that drives the REPL page in headless Chrome or Edge.
- The core is `no_std` with `alloc`. The `std` feature adds the `site` and `wasm` modules and the binary.

//! The post-`cargo doc` step for tpt-dev-doc.
//!
//! Usage: `tpt-dev-doc-site <doc-dir> <source> <title>`
//!
//! `<doc-dir>` is the crate's rustdoc directory, such as `target/doc/my_crate`.
//! `<source>` is a Rust file, whose doc comments hold the examples, or a
//! Markdown file. The step injects a static examples block into every HTML page
//! under `<doc-dir>`, then writes the opt-in REPL page `examples-repl.html`. See
//! the crate README for the commands.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [doc_dir, source, title] = args.as_slice() else {
        eprintln!("usage: tpt-dev-doc-site <doc-dir> <source.rs|source.md> <title>");
        return ExitCode::from(2);
    };
    match tpt_dev_doc::site::run_post_doc_step(Path::new(doc_dir), Path::new(source), title) {
        Ok(report) => {
            println!(
                "tpt-dev-doc-site: {} examples; injected into {} page(s), {} already had it, {} without </body>; REPL at {}",
                report.examples,
                report.injection.injected.len(),
                report.injection.unchanged.len(),
                report.injection.missing_marker.len(),
                report.repl_page.display()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("tpt-dev-doc-site: {err}");
            ExitCode::FAILURE
        }
    }
}

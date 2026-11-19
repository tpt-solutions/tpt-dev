//! Files on disk: a static examples page, and injection into rustdoc output.
//!
//! These functions need `std`. Run them as a step after `cargo doc`, from an
//! `xtask` or a small binary. Do not run them from `build.rs`: cargo runs a
//! build script before rustdoc documents the crate, and rustdoc overwrites what
//! the build script wrote into `target/doc`. This was checked with `cargo doc`
//! on a probe crate.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::extract::{Example, doc_comments, extract};
use crate::render::{escape, example_section, inject_before, page};
use crate::wasm::{RUNNER_JS, compile, wasm_source};

/// The closing tag that rustdoc writes on every page. The examples block goes
/// in front of it.
const INJECT_MARKER: &str = "</body>";

/// File name of the REPL page, written in the rustdoc crate directory.
pub const REPL_FILE_NAME: &str = "examples-repl.html";

/// Content-Security-Policy for the REPL page. It allows inline script, blob
/// workers, WebAssembly, and fetches from its own origin. Not checked in a
/// browser.
const REPL_CSP: &str = "default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval'; \
     worker-src blob:; connect-src 'self'; style-src 'unsafe-inline'";

/// What [`inject_into_rustdoc`] did to each HTML page it found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Injection {
    /// Pages that now contain the fragment.
    pub injected: Vec<PathBuf>,
    /// Pages that already contain the fragment, so were left as they are.
    pub unchanged: Vec<PathBuf>,
    /// Pages without the marker. Rustdoc's layout may have changed.
    pub missing_marker: Vec<PathBuf>,
}

/// Inserts `fragment` before `marker` in every `.html` file under `doc_dir`.
///
/// Pass the crate's directory, such as `target/doc/my_crate`, and a marker such
/// as `</body>`. A page that already contains `fragment` is not changed again,
/// so the call can be repeated. Other files are not read.
pub fn inject_into_rustdoc(doc_dir: &Path, marker: &str, fragment: &str) -> io::Result<Injection> {
    let mut report = Injection::default();
    let mut pending = vec![doc_dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let mut entries = fs::read_dir(&dir)?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_by_key(fs::DirEntry::path);
        for entry in entries {
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "html") {
                let text = fs::read_to_string(&path)?;
                if text.contains(fragment) {
                    report.unchanged.push(path);
                } else if let Some(updated) = inject_before(&text, marker, fragment) {
                    fs::write(&path, updated)?;
                    report.injected.push(path);
                } else {
                    report.missing_marker.push(path);
                }
            }
        }
    }
    Ok(report)
}

/// Writes a static examples page to `doc_dir/file_name` and returns its path.
///
/// `file_name` must be a plain `.html` name with no directory part. The
/// directory is created if needed. The page is the one [`page`] returns, so it
/// has no script and no network reference.
pub fn write_examples_page(
    doc_dir: &Path,
    file_name: &str,
    title: &str,
    examples: &[Example],
) -> io::Result<PathBuf> {
    check_plain_html_name(file_name)?;
    fs::create_dir_all(doc_dir)?;
    let path = doc_dir.join(file_name);
    fs::write(&path, page(title, examples))?;
    Ok(path)
}

fn check_plain_html_name(file_name: &str) -> io::Result<()> {
    let plain = !file_name.is_empty()
        && file_name.ends_with(".html")
        && !file_name.contains(['/', '\\', ':']);
    if plain {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "file name must be a plain .html name",
        ))
    }
}

/// The static examples block that [`run_post_doc_step`] injects into rustdoc
/// pages. It is a collapsed `<details>` element with the same escaped sections
/// as [`page`], so it needs no script. Empty when there are no examples.
pub fn examples_fragment(examples: &[Example]) -> String {
    if examples.is_empty() {
        return String::new();
    }
    let mut out =
        String::from("<details class=\"tpt-dev-examples\">\n<summary>Examples</summary>\n");
    for example in examples {
        out.push_str(&example_section(example));
    }
    out.push_str("</details>\n");
    out
}

/// The REPL page. Each runnable example gets a Run button whose `data-wasm`
/// attribute is the relative path `wasm_dir/example-N.wasm`, where N is the
/// example's index in `examples`. Examples that are not runnable are shown as
/// static sections.
///
/// The page has one script, which is inline. It holds [`RUNNER_JS`] and the
/// code that wires the buttons to it. The page has no other script, no link,
/// image or frame, and no URL. `wasm_dir` must be a relative directory name.
/// Writing the `.wasm` files is the job of [`write_repl_page`].
pub fn repl_page(title: &str, examples: &[Example], wasm_dir: &str) -> String {
    let mut out = String::from(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta http-equiv=\"Content-Security-Policy\" content=\"",
    );
    out.push_str(REPL_CSP);
    out.push_str("\">\n<title>");
    out.push_str(&escape(title));
    out.push_str(
        "</title>\n<style>\n\
         :root { --bg: #ffffff; --fg: #1d1d1f; --muted: #6e6e73; --code-bg: #f5f5f7; }\n\
         @media (prefers-color-scheme: dark) {\n\
           :root:not([data-theme=\"light\"]) { --bg: #111113; --fg: #f2f2f4; --muted: #9a9aa3; --code-bg: #1c1c20; }\n\
         }\n\
         :root[data-theme=\"dark\"] { --bg: #111113; --fg: #f2f2f4; --muted: #9a9aa3; --code-bg: #1c1c20; }\n\
         body { background: var(--bg); color: var(--fg); margin: 0; padding: 16px; font: 16px/1.5 system-ui, sans-serif; }\n\
         main { max-width: 860px; margin: 0 auto; }\n\
         pre { background: var(--code-bg); padding: 12px; overflow-x: auto; border-radius: 6px; }\n\
         .meta { color: var(--muted); font-size: 13px; margin: 16px 0 4px; }\n\
         button { font: inherit; padding: 4px 14px; }\n\
         output { display: block; margin-top: 6px; color: var(--muted); }\n\
         </style>\n</head>\n<body>\n<main>\n<h1>",
    );
    out.push_str(&escape(title));
    out.push_str(
        "</h1>\n<p class=\"meta\">Run compiles nothing. Each example was built ahead of \
         time to WebAssembly and runs in a Web Worker in this browser, stopped after 5 seconds.</p>\n",
    );
    for (index, example) in examples.iter().enumerate() {
        if wasm_source(example).is_none() {
            out.push_str(&example_section(example));
            continue;
        }
        out.push_str("<section class=\"example\" data-mode=\"run\">\n<p class=\"meta\">line ");
        out.push_str(&example.line.to_string());
        out.push_str(": compiled and run</p>\n<pre><code class=\"language-rust\">");
        out.push_str(&escape(example.shown.trim_end()));
        out.push_str("</code></pre>\n<button type=\"button\" data-wasm=\"");
        out.push_str(&escape(&format!("{wasm_dir}/example-{index}.wasm")));
        out.push_str("\">Run</button>\n<output></output>\n</section>\n");
    }
    out.push_str("</main>\n<script type=\"module\">\n");
    out.push_str(RUNNER_JS);
    out.push_str(
        "\nfor (const button of document.querySelectorAll(\"button[data-wasm]\")) {\n\
         const output = button.nextElementSibling;\n\
         button.addEventListener(\"click\", async () => {\n\
         button.disabled = true;\n\
         output.textContent = \"running\";\n\
         const result = await runExample(button.dataset.wasm);\n\
         if (result.status === \"ok\") output.textContent = \"passed\";\n\
         else if (result.status === \"timeout\") output.textContent = \"stopped: time limit reached\";\n\
         else output.textContent = \"failed: \" + result.detail;\n\
         button.disabled = false;\n\
         });\n}\n</script>\n</body>\n</html>\n",
    );
    out
}

/// Compiles every runnable example into `doc_dir/<stem>-wasm/`, then writes the
/// REPL page `doc_dir/file_name` that refers to those files by relative path.
///
/// `file_name` must be a plain `.html` name. The `-wasm` directory is replaced
/// on each run. A compile error stops the step and names the example's line,
/// and no page is written. This compiles example source with `rustc`, so use
/// it on trusted documentation only.
pub fn write_repl_page(
    doc_dir: &Path,
    file_name: &str,
    title: &str,
    examples: &[Example],
) -> io::Result<PathBuf> {
    check_plain_html_name(file_name)?;
    let stem = &file_name[..file_name.len() - ".html".len()];
    let wasm_dir_name = format!("{stem}-wasm");
    let wasm_dir = doc_dir.join(&wasm_dir_name);
    if wasm_dir.exists() {
        fs::remove_dir_all(&wasm_dir)?;
    }
    fs::create_dir_all(&wasm_dir)?;
    let build = wasm_dir.join("build");
    let compiled = compile_runnable(examples, &build, &wasm_dir);
    let _ = fs::remove_dir_all(&build);
    compiled?;
    let path = doc_dir.join(file_name);
    fs::write(&path, repl_page(title, examples, &wasm_dir_name))?;
    Ok(path)
}

fn compile_runnable(examples: &[Example], build: &Path, wasm_dir: &Path) -> io::Result<()> {
    for (index, example) in examples.iter().enumerate() {
        if wasm_source(example).is_none() {
            continue;
        }
        let out = wasm_dir.join(format!("example-{index}.wasm"));
        compile(example, build, &out).map_err(|err| {
            io::Error::new(
                err.kind(),
                format!("example at line {}: {err}", example.line),
            )
        })?;
    }
    Ok(())
}

/// What [`run_post_doc_step`] wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostDocReport {
    /// Rustdoc pages that got the examples block on this run. The REPL page is
    /// left out, since it is rewritten after injection.
    pub injection: Injection,
    /// How many examples the source holds.
    pub examples: usize,
    /// The REPL page that was written.
    pub repl_page: PathBuf,
}

/// The step to run after `cargo doc`. It injects the static examples block
/// into every rustdoc page under `doc_dir`, then writes the REPL page
/// `doc_dir/`[`REPL_FILE_NAME`].
///
/// `source` is a Rust file, whose `///` and `//!` comments are read, or a
/// Markdown file, which is read as it is. The REPL page is written after the
/// injection, so it does not get the block. Running the step again is safe.
pub fn run_post_doc_step(doc_dir: &Path, source: &Path, title: &str) -> io::Result<PostDocReport> {
    let text = fs::read_to_string(source)?;
    let is_markdown = source.extension().is_some_and(|ext| ext == "md");
    let markdown = if is_markdown {
        text
    } else {
        doc_comments(&text)
    };
    let examples = extract(&markdown);
    let fragment = examples_fragment(&examples);
    let mut injection = if fragment.is_empty() {
        Injection::default()
    } else {
        inject_into_rustdoc(doc_dir, INJECT_MARKER, &fragment)?
    };
    let repl_page = write_repl_page(doc_dir, REPL_FILE_NAME, title, &examples)?;
    injection.injected.retain(|path| path != &repl_page);
    injection.unchanged.retain(|path| path != &repl_page);
    injection.missing_marker.retain(|path| path != &repl_page);
    Ok(PostDocReport {
        injection,
        examples: examples.len(),
        repl_page,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::extract::extract;
    use alloc::format;
    use alloc::vec;

    /// A scratch directory unique to this test, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("tpt-dev-doc-{}-{}", std::process::id(), name));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A rustdoc-shaped tree: an index, a nested item page, a page with no
    /// `</body>`, and a non-HTML file that must not be touched.
    fn rustdoc_like(root: &Path) {
        let krate = root.join("my_crate");
        fs::create_dir_all(krate.join("sub")).unwrap();
        fs::write(
            krate.join("index.html"),
            "<html><body><main>crate</main></body></html>",
        )
        .unwrap();
        fs::write(
            krate.join("sub").join("fn.run.html"),
            "<html><body><main>fn</main></body></html>",
        )
        .unwrap();
        fs::write(krate.join("odd.html"), "<html>no body close</html>").unwrap();
        fs::write(krate.join("search-index.js"), "</body> in js").unwrap();
    }

    #[test]
    fn injects_into_every_html_page_under_the_crate_dir() {
        let scratch = Scratch::new("inject");
        rustdoc_like(&scratch.0);
        let krate = scratch.0.join("my_crate");
        let fragment = "<aside>examples</aside>";

        let report = inject_into_rustdoc(&krate, "</body>", fragment).unwrap();

        assert_eq!(report.injected.len(), 2);
        assert_eq!(report.missing_marker, vec![krate.join("odd.html")]);
        assert!(report.unchanged.is_empty());
        let index = fs::read_to_string(krate.join("index.html")).unwrap();
        assert_eq!(
            index,
            "<html><body><main>crate</main><aside>examples</aside></body></html>"
        );
        let js = fs::read_to_string(krate.join("search-index.js")).unwrap();
        assert_eq!(js, "</body> in js", "non-HTML files are left alone");
    }

    #[test]
    fn a_second_run_leaves_injected_pages_unchanged() {
        let scratch = Scratch::new("twice");
        rustdoc_like(&scratch.0);
        let krate = scratch.0.join("my_crate");
        let fragment = "<aside>examples</aside>";
        inject_into_rustdoc(&krate, "</body>", fragment).unwrap();
        let once = fs::read_to_string(krate.join("index.html")).unwrap();

        let report = inject_into_rustdoc(&krate, "</body>", fragment).unwrap();

        assert!(report.injected.is_empty());
        assert_eq!(report.unchanged.len(), 2);
        assert_eq!(fs::read_to_string(krate.join("index.html")).unwrap(), once);
    }

    #[test]
    fn writes_the_static_page_next_to_rustdoc_output() {
        let scratch = Scratch::new("write");
        let krate = scratch.0.join("my_crate");
        let examples = extract("Adds.\n\n```rust\nassert_eq!(2 + 2, 4);\n```\n");

        let path = write_examples_page(&krate, "examples.html", "Arithmetic", &examples).unwrap();

        assert_eq!(path, krate.join("examples.html"));
        let html = fs::read_to_string(&path).unwrap();
        assert!(html.contains("<h1>Arithmetic</h1>"));
        assert!(html.contains("assert_eq!(2 + 2, 4);"));
    }

    #[test]
    fn rejects_file_names_that_leave_the_directory() {
        let scratch = Scratch::new("names");
        for bad in [
            "../x.html",
            "a/b.html",
            "a\\b.html",
            "x.txt",
            "",
            "C:x.html",
        ] {
            let err = write_examples_page(&scratch.0, bad, "t", &[]).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{bad}");
        }
    }

    /// One runnable example, one ignored, and one with `fn main` (not runnable).
    fn mixed_examples() -> Vec<Example> {
        extract(
            "Text.\n\n```rust\nassert_eq!(1 + 1, 2);\n```\n\n```rust,ignore\nlet x = 1 < 2;\n```\n\n\
             ```rust\nfn main() {}\n```\n",
        )
    }

    #[test]
    fn the_repl_page_refers_only_to_its_inline_runner_and_relative_wasm() {
        let html = repl_page("Demo", &mixed_examples(), "demo-wasm");

        // No URL of any kind, and no element that fetches or embeds anything.
        for banned in [
            "://", "src=", "href=", "<link", "<img", "<iframe", "<object", "<base", "@import",
            "url(",
        ] {
            assert!(!html.contains(banned), "found {banned:?}");
        }
        // One script, inline, holding the runner.
        assert_eq!(html.matches("<script").count(), 1);
        assert!(html.contains("<script type=\"module\">"));
        assert!(html.contains(RUNNER_JS));
        // Only the runnable example has a button, and it names a relative path.
        assert_eq!(html.matches("<button").count(), 1);
        let refs: Vec<&str> = html
            .match_indices("data-wasm=\"")
            .map(|(at, marker)| {
                let rest = &html[at + marker.len()..];
                &rest[..rest.find('"').unwrap()]
            })
            .collect();
        assert_eq!(refs, ["demo-wasm/example-0.wasm"]);
        assert!(!refs[0].contains(':') && !refs[0].starts_with('/'));
        assert!(html.contains("content=\"default-src 'none';"));
    }

    #[test]
    fn the_runner_cannot_end_the_inline_script_early() {
        assert!(!RUNNER_JS.contains("</"));
    }

    #[test]
    fn the_repl_page_escapes_example_text_and_title() {
        let examples = extract("```rust\nlet a = b < c;\n</script>\n```\n");
        let html = repl_page("<T>", &examples, "x-wasm");
        assert!(html.contains("<title>&lt;T&gt;</title>"));
        assert!(html.contains("let a = b &lt; c;"));
        // The example's own closing tag is escaped; the page's only
        // `</script>` is the one that closes its inline runner.
        assert!(html.contains("&lt;/script&gt;"));
        assert_eq!(html.matches("</script>").count(), 1);
    }

    #[test]
    fn the_static_fragment_has_no_script_and_is_empty_without_examples() {
        let fragment = examples_fragment(&mixed_examples());
        assert!(fragment.starts_with("<details class=\"tpt-dev-examples\">"));
        assert!(!fragment.contains("<script"));
        assert!(!fragment.contains("button"));
        assert_eq!(examples_fragment(&[]), "");
    }

    #[test]
    fn the_post_doc_step_injects_and_writes_the_repl_on_a_rustdoc_tree() {
        let scratch = Scratch::new("step");
        rustdoc_like(&scratch.0);
        let krate = scratch.0.join("my_crate");
        let source = scratch.0.join("lib.rs");
        fs::write(
            &source,
            "/// Adds.\n///\n/// ```rust\n/// assert_eq!(2 + 2, 4);\n/// ```\npub fn add() {}\n",
        )
        .unwrap();

        let first = run_post_doc_step(&krate, &source, "My crate").unwrap();

        assert_eq!(first.examples, 1);
        assert_eq!(first.injection.injected.len(), 2);
        assert_eq!(first.injection.missing_marker, vec![krate.join("odd.html")]);
        let index = fs::read_to_string(krate.join("index.html")).unwrap();
        assert!(index.contains("<details class=\"tpt-dev-examples\">"));
        assert!(!index.contains("<script"));
        let wasm = fs::read(krate.join("examples-repl-wasm").join("example-0.wasm")).unwrap();
        assert_eq!(&wasm[..4], b"\0asm");
        let repl = fs::read_to_string(&first.repl_page).unwrap();
        assert_eq!(first.repl_page, krate.join(REPL_FILE_NAME));
        assert!(repl.contains("data-wasm=\"examples-repl-wasm/example-0.wasm\""));
        assert!(!repl.contains("tpt-dev-examples"));
        assert!(!krate.join("examples-repl-wasm").join("build").exists());

        let second = run_post_doc_step(&krate, &source, "My crate").unwrap();

        assert!(second.injection.injected.is_empty());
        assert_eq!(second.injection.unchanged.len(), 2);
        let repl = fs::read_to_string(&second.repl_page).unwrap();
        assert!(!repl.contains("tpt-dev-examples"));
    }

    #[test]
    fn a_failing_example_stops_the_step_and_writes_no_repl_page() {
        let scratch = Scratch::new("step-fail");
        let krate = scratch.0.join("my_crate");
        fs::create_dir_all(&krate).unwrap();
        let source = scratch.0.join("README.md");
        fs::write(&source, "Bad.\n\n```rust\nlet x: u8 = \"no\";\n```\n").unwrap();

        let err = run_post_doc_step(&krate, &source, "Bad").unwrap_err();

        assert!(err.to_string().contains("example at line 3"), "{err}");
        assert!(!krate.join(REPL_FILE_NAME).exists());
    }
}

//! Files on disk: a static examples page, and injection into rustdoc output.
//!
//! These functions need `std`. Run them as a step after `cargo doc`, from an
//! `xtask` or a small binary. Do not run them from `build.rs`: cargo runs a
//! build script before rustdoc documents the crate, and rustdoc overwrites what
//! the build script wrote into `target/doc`. This was checked with `cargo doc`
//! on a probe crate.

use alloc::vec;
use alloc::vec::Vec;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::extract::Example;
use crate::render::{inject_before, page};

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
    let plain = !file_name.is_empty()
        && file_name.ends_with(".html")
        && !file_name.contains(['/', '\\', ':']);
    if !plain {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "file name must be a plain .html name",
        ));
    }
    fs::create_dir_all(doc_dir)?;
    let path = doc_dir.join(file_name);
    fs::write(&path, page(title, examples))?;
    Ok(path)
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
}

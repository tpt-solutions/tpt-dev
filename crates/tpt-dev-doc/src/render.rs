//! Static HTML for examples, and injection of a fragment into a rustdoc page.
//!
//! The output has no script and makes no network request. It is a plain page,
//! so it can be hosted anywhere that serves files.

use alloc::string::{String, ToString};

use crate::extract::{Example, Mode};

/// Escapes text for use in HTML element content or a double-quoted attribute.
///
/// # Examples
///
/// ```
/// use tpt_dev_doc::escape;
///
/// assert_eq!(escape("<b>&"), "&lt;b&gt;&amp;");
/// ```
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Run => "run",
        Mode::NoRun => "no_run",
        Mode::CompileFail => "compile_fail",
        Mode::ShouldPanic => "should_panic",
        Mode::Ignore => "ignore",
        Mode::Text => "text",
    }
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Run => "compiled and run",
        Mode::NoRun => "compiled, not run",
        Mode::CompileFail => "must not compile",
        Mode::ShouldPanic => "must panic",
        Mode::Ignore => "not compiled",
        Mode::Text => "text",
    }
}

/// One example, as a section with its line number and mode.
///
/// # Examples
///
/// ```
/// use tpt_dev_doc::{example_section, extract};
///
/// let examples = extract("```rust\nassert!(1 < 2);\n```\n");
/// let html = example_section(&examples[0]);
/// assert!(html.contains("assert!(1 &lt; 2);"));
/// ```
pub fn example_section(example: &Example) -> String {
    let language = if example.mode == Mode::Text {
        "text"
    } else {
        "rust"
    };
    let mut out = String::new();
    out.push_str("<section class=\"example\" data-mode=\"");
    out.push_str(mode_name(example.mode));
    out.push_str("\">\n<p class=\"meta\">line ");
    out.push_str(&example.line.to_string());
    out.push_str(": ");
    out.push_str(mode_label(example.mode));
    out.push_str("</p>\n<pre><code class=\"language-");
    out.push_str(language);
    out.push_str("\">");
    out.push_str(&escape(example.shown.trim_end()));
    out.push_str("</code></pre>\n</section>\n");
    out
}

/// A complete page showing `examples` under `title`.
///
/// # Examples
///
/// ```
/// use tpt_dev_doc::page;
///
/// let html = page("Examples", &[]);
/// assert!(html.starts_with("<!doctype html>"));
/// assert!(html.contains("Examples"));
/// ```
pub fn page(title: &str, examples: &[Example]) -> String {
    let mut out = String::from(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>",
    );
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
         </style>\n</head>\n<body>\n<main>\n<h1>",
    );
    out.push_str(&escape(title));
    out.push_str("</h1>\n");
    for example in examples {
        out.push_str(&example_section(example));
    }
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

/// Inserts `fragment` before the first occurrence of `marker` in `page`.
///
/// Returns `None` when the marker is not in the page, so the caller can report
/// that rustdoc's layout has changed.
///
/// # Examples
///
/// ```
/// use tpt_dev_doc::inject_before;
///
/// let html = "<html><body>text</body></html>";
/// assert_eq!(
///     inject_before(html, "</body>", "<p>note</p>").unwrap(),
///     "<html><body>text<p>note</p></body></html>"
/// );
/// assert!(inject_before(html, "</main>", "x").is_none());
/// ```
pub fn inject_before(page: &str, marker: &str, fragment: &str) -> Option<String> {
    let at = page.find(marker)?;
    let mut out = String::with_capacity(page.len() + fragment.len());
    out.push_str(&page[..at]);
    out.push_str(fragment);
    out.push_str(&page[at..]);
    Some(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::extract::extract;

    #[test]
    fn escapes_markup_and_quotes() {
        assert_eq!(
            escape("a < b && \"c\" > 'd'"),
            "a &lt; b &amp;&amp; &quot;c&quot; &gt; &#39;d&#39;"
        );
    }

    #[test]
    fn example_code_is_escaped_in_the_page() {
        let examples = extract("```rust\nif a < b && c > d {}\n```\n");
        let html = page("Demo", &examples);
        assert!(html.contains("if a &lt; b &amp;&amp; c &gt; d {}"));
        assert!(!html.contains("if a < b"));
    }

    #[test]
    fn the_title_is_escaped_and_the_page_is_complete() {
        let html = page("<Crate> & more", &[]);
        assert!(html.contains("<title>&lt;Crate&gt; &amp; more</title>"));
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.trim_end().ends_with("</html>"));
        assert!(!html.contains("<script"));
    }

    #[test]
    fn the_static_page_has_no_script_or_network_reference() {
        // Kept static on purpose: docs.rs can disable scripts. Every example
        // mode is included, so the page covers every branch of the renderer.
        let examples = extract(
            "Text.\n\n```rust\nlet a = 1 < 2;\n```\n\n```rust,ignore\nb\n```\n\n\
             ```rust,should_panic\nc\n```\n\n```toml\nname = \"x\"\n```\n",
        );
        let html = page("Hosted", &examples);
        for banned in [
            "<script",
            "<iframe",
            "<link",
            "<img",
            "<object",
            "<embed",
            "<base",
            "<form",
            "<svg",
            "<meta http-equiv",
            "src=",
            "href=",
            "action=",
            "srcset",
            "url(",
            "@import",
            "http",
            "//",
        ] {
            assert!(!html.contains(banned), "found {banned:?} in the page");
        }
        // Event handler attributes such as onload="..." or onclick='...'.
        let bytes = html.as_bytes();
        for (at, _) in html.match_indices(" on") {
            let name_end = bytes[at + 1..]
                .iter()
                .position(|b| !b.is_ascii_alphabetic())
                .map_or(bytes.len(), |n| at + 1 + n);
            assert_ne!(bytes.get(name_end), Some(&b'='), "event handler at {at}");
        }
    }

    #[test]
    fn sections_record_the_mode_and_line() {
        let examples = extract("text\n\n```rust,ignore\nx\n```\n");
        let section = example_section(&examples[0]);
        assert!(section.contains("data-mode=\"ignore\""));
        assert!(section.contains("line 3: not compiled"));
    }

    #[test]
    fn injection_goes_before_the_marker() {
        let page = "<html><body>content</body></html>";
        let out = inject_before(page, "</body>", "<p>note</p>").unwrap();
        assert_eq!(out, "<html><body>content<p>note</p></body></html>");
    }

    #[test]
    fn injection_reports_a_missing_marker() {
        assert_eq!(inject_before("<html></html>", "</body>", "x"), None);
    }
}

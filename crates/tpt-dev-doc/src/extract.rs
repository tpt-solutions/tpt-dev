//! Finding examples in Markdown, and in the doc comments of Rust source.
//!
//! The rules follow rustdoc: an untagged fence or one tagged `rust` is Rust,
//! `ignore`, `no_run`, `compile_fail` and `should_panic` change how it is
//! treated, and a line that starts with `# ` is hidden from the reader but still
//! compiled.

use alloc::string::String;
use alloc::vec::Vec;

/// How an example is meant to be treated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Compile and run.
    Run,
    /// Compile, but do not run.
    NoRun,
    /// Must fail to compile.
    CompileFail,
    /// Must panic when run.
    ShouldPanic,
    /// Shown to the reader, not compiled.
    Ignore,
    /// Not Rust.
    Text,
}

/// One code block found in the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Example {
    /// The line of the opening fence, counting from 1.
    pub line: usize,
    /// How the example is meant to be treated.
    pub mode: Mode,
    /// The code the compiler sees: hidden lines included, with their `# ` removed.
    pub source: String,
    /// The code a reader sees: hidden lines removed.
    pub shown: String,
}

/// Finds every fenced code block in `markdown`.
///
/// A block with no closing fence runs to the end of the input.
pub fn extract(markdown: &str) -> Vec<Example> {
    let mut examples = Vec::new();
    let mut lines = markdown.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let Some(info) = line.trim_start().strip_prefix("```") else {
            continue;
        };
        let (mode, is_rust) = classify(info.trim());
        let mut source = String::new();
        let mut shown = String::new();
        for (_, body) in lines.by_ref() {
            if body.trim() == "```" {
                break;
            }
            if is_rust {
                push_rust_line(&mut source, &mut shown, body);
            } else {
                // A non-Rust block is shown as written, and never compiled.
                push_line(&mut shown, body);
            }
        }
        examples.push(Example {
            line: index + 1,
            mode: if is_rust { mode } else { Mode::Text },
            source,
            shown,
        });
    }
    examples
}

/// Returns the Rust source of the doc comments in `source`, with the `///` and
/// `//!` markers removed. Other lines are dropped.
pub fn doc_comments(source: &str) -> String {
    let mut out = String::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let text = trimmed
            .strip_prefix("///")
            .or_else(|| trimmed.strip_prefix("//!"));
        if let Some(text) = text {
            push_line(&mut out, text.strip_prefix(' ').unwrap_or(text));
        }
    }
    out
}

/// Reads a fence's info string. Returns the mode, and whether the block is Rust.
fn classify(info: &str) -> (Mode, bool) {
    let tags: Vec<&str> = info
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    let is_rust = tags.is_empty() || tags.contains(&"rust");
    let has = |tag: &str| tags.contains(&tag);
    let mode = if has("ignore") {
        Mode::Ignore
    } else if has("no_run") {
        Mode::NoRun
    } else if has("compile_fail") {
        Mode::CompileFail
    } else if has("should_panic") {
        Mode::ShouldPanic
    } else {
        Mode::Run
    };
    (mode, is_rust)
}

/// Adds one line of a Rust block to the compiled source and, unless it is
/// hidden, to the shown text. A leading `##` is a literal `#`, so it is
/// unescaped in both.
fn push_rust_line(source: &mut String, shown: &mut String, line: &str) {
    let trimmed = line.trim_start();
    if trimmed == "#" {
        push_line(source, "");
    } else if let Some(rest) = trimmed.strip_prefix("# ") {
        push_line(source, rest);
    } else if trimmed.starts_with("##") {
        let text = line.replacen('#', "", 1);
        push_line(source, &text);
        push_line(shown, &text);
    } else {
        push_line(source, line);
        push_line(shown, line);
    }
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn modes(md: &str) -> Vec<Mode> {
        extract(md).into_iter().map(|e| e.mode).collect()
    }

    #[test]
    fn finds_blocks_with_their_line_numbers() {
        let md = "intro\n\n```rust\nlet x = 1;\n```\n";
        let found = extract(md);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
        assert_eq!(found[0].mode, Mode::Run);
        assert_eq!(found[0].source, "let x = 1;\n");
    }

    #[test]
    fn tags_set_the_mode_as_rustdoc_reads_them() {
        let md = "```rust,ignore\na\n```\n\
                  ```rust,no_run\nb\n```\n\
                  ```rust,compile_fail\nc\n```\n\
                  ```rust,should_panic\nd\n```\n\
                  ```\ne\n```\n";
        assert_eq!(
            modes(md),
            [
                Mode::Ignore,
                Mode::NoRun,
                Mode::CompileFail,
                Mode::ShouldPanic,
                Mode::Run
            ]
        );
    }

    #[test]
    fn other_languages_are_text_and_not_compiled() {
        let md = "```toml\nname = \"x\"\n```\n";
        let found = extract(md);
        assert_eq!(found[0].mode, Mode::Text);
        assert!(found[0].source.is_empty());
    }

    #[test]
    fn hidden_lines_are_compiled_but_not_shown() {
        let md = "```rust\n# use std::fmt;\nlet x = 2;\n# assert_eq!(x, 2);\n```\n";
        let found = &extract(md)[0];
        assert_eq!(
            found.source,
            "use std::fmt;\nlet x = 2;\nassert_eq!(x, 2);\n"
        );
        assert_eq!(found.shown, "let x = 2;\n");
    }

    #[test]
    fn a_double_hash_is_a_literal_hash() {
        let md = "```rust\n## not hidden\n```\n";
        let found = &extract(md)[0];
        assert_eq!(found.source, "# not hidden\n");
        assert_eq!(found.shown, "# not hidden\n");
    }

    #[test]
    fn an_unclosed_block_runs_to_the_end() {
        let md = "```rust\nlet y = 3;\n";
        assert_eq!(extract(md)[0].source, "let y = 3;\n");
    }

    #[test]
    fn doc_comments_yield_the_markdown_inside_them() {
        let src =
            "fn a() {}\n/// Adds.\n///\n/// ```\n/// assert_eq!(1, 1);\n/// ```\n//! Module doc.\n";
        let md = doc_comments(src);
        assert_eq!(md, "Adds.\n\n```\nassert_eq!(1, 1);\n```\nModule doc.\n");
        let found = extract(&md);
        assert_eq!(found[0].source, "assert_eq!(1, 1);\n");
    }
}

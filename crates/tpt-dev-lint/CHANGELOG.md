# Changelog

All notable changes to `tpt-dev-lint` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows [Semantic Versioning](https://github.com/tpt-solutions/tpt-dev/blob/master/VERSIONING.md).

## [Unreleased]

### Added
- `Rule` trait, `Linter`, `Finding`, `Diagnostic` and `Severity` for running syntax-tree lint rules without compiler plugins.
- Built-in rule `no-unwrap` (warn): flags `.unwrap()` and `.expect(..)`.
- Built-in rule `no-todo-macros` (warn): flags `todo!` and `unimplemented!`.
- Built-in rule `safety-comment` (deny): requires a `SAFETY` comment near each `unsafe` block.
- Built-in rule `doc-example` (warn): flags public functions and inherent methods whose docs have no fenced code block.
- Config file with `rule = allow|warn|deny` lines, with line-numbered errors and rejection of unknown rule names.
- `tpt-dev-lint` command with `--config`, Clippy-style output, and exit codes 1 (deny finding) and 2 (usage or read error).
- `#[test]` functions and `#[cfg(test)]` modules are skipped by all rules.

# tpt-dev-dep

Dependency graph analyzer for cycles, license violations and bloat.

## Overview

`tpt-dev-dep` reads a Cargo dependency graph, from `Cargo.lock` or from `cargo metadata --format-version 1` output, and reports three things: dependency cycles, packages whose license is not on an allow list, and the transitive dependency count of each package.

It is for keeping a workspace's dependency tree small and acceptable, in CI or by hand. It ships as a library and as a command-line tool of the same name. It is a host tool and has no dependencies, including on other `tpt-dev-*` crates.

## Features

- **Cycle detection:** Tarjan's algorithm finds strongly connected components that contain a cycle.
- **License policy:** SPDX-style expressions are checked against an allow list. `OR` (or `/`) needs one allowed option, `AND` needs all, and `WITH` exceptions are ignored. Packages with no license are reported as `missing`.
- **Bloat:** each package's transitive dependency count, largest first.
- **Metadata input:** `cargo metadata` output is parsed with a strict JSON parser (nesting depth limited to `MAX_DEPTH`, 128). Its resolve graph gives the edges Cargo uses, each tagged as normal, dev or build. Dev and build edges are followed only when asked.
- **Command-line tool:** prints the findings and sets an exit status for CI.

## Installation

```sh
cargo add tpt-dev-dep
```

| Feature | Enables | Default |
| --- | --- | --- |
| (none) | The crate has no feature flags. | n/a |

The crate is host-only (`std`, file reading). CI excludes it from the `thumbv7em-none-eabihf` build.

## Quick start

```rust
use tpt_dev_dep::{Policy, Scope, analyse, parse_lock};

let lock = "\
[[package]]
name = \"app\"
version = \"1.0.0\"
dependencies = [\"lib\"]

[[package]]
name = \"lib\"
version = \"0.1.0\"
";
let graph = parse_lock(lock).unwrap();

let licenses = [
    ("app".to_string(), "MIT".to_string()),
    ("lib".to_string(), "GPL-3.0-only".to_string()),
];
let policy = Policy { allow: vec!["MIT".to_string()] };

let report = analyse(&graph, &licenses, &policy, Scope::default());
assert!(report.cycles.is_empty());
assert_eq!(report.license_violations, [("lib".to_string(), "GPL-3.0-only".to_string())]);
assert_eq!(report.transitive[0], ("app".to_string(), 1));
assert!(!report.is_clean());
```

### Command line

```sh
cargo run -p tpt-dev-dep -- Cargo.lock licenses.txt --allow MIT --allow Apache-2.0
cargo metadata --format-version 1 > metadata.json
cargo run -p tpt-dev-dep -- --metadata metadata.json licenses.txt --allow MIT --include-dev
```

The usage is `tpt-dev-dep <Cargo.lock> [licenses.txt] [--allow ID]...`, or `tpt-dev-dep --metadata <file> [licenses.txt] [--allow ID]... [--include-dev] [--include-build]`. The tool prints each cycle and license violation, then the five largest transitive trees, then `ok: N packages` if clean. It exits 1 on a cycle or a license violation, and 2 on a usage, read or parse error.

`licenses.txt` has one `name license-expression` pair per line. Cargo's lock file does not record licenses, so supply them from `cargo metadata` or a `cargo deny` report. With no `licenses.txt`, every package is reported as `missing`.

## Design notes and guarantees

- **Strict inputs.** Malformed JSON, a missing or mistyped metadata field, an unknown dependency kind and an unparseable lock entry are errors, not skipped.
- **Edge kinds are explicit.** `Scope` chooses whether dev and build edges are followed. Normal edges are always followed.
- **Identity is checked.** A resolve graph with two packages of the same name and version is rejected, because the edges could not be told apart.

## Limitations and scope

- **Lock file subset.** The parser reads `[[package]]` tables with `name`, `version`, `source` and `dependencies`. Other fields are ignored, and unusual dependency syntax is an error. Lock-file edges are all normal, because the file does not record kinds.
- **Metadata without deps.** Output from `--no-deps` has no resolve graph. Its edges come from declared dependencies and are matched by name, so they may be ambiguous when one name has several versions.
- **Licenses are supplied.** The tool does not read them from metadata or look them up, so a missing license is reported rather than checked.
- **No parentheses in expressions.** Expressions with grouping are not parsed, and they count as not allowed.
- **Text output only.** JSON output is not written.
- **Host tool.** It uses `std`, and CI excludes it from the bare-metal build.

## Related crates

`tpt-dev-dep` does not depend on, and is not depended on by, any other workspace crate. [`tpt-dev-lint`](https://docs.rs/tpt-dev-lint) is the other source-checking host tool.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

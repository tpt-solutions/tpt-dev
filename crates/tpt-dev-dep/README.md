# tpt-dev-dep

A dependency graph analyzer for a Cargo workspace. It reads `Cargo.lock`, finds dependency cycles, checks licenses against a policy, and reports the transitive size of each package.

- **Cycle detection:** Tarjan's algorithm finds strongly connected components that contain a cycle.
- **License policy:** SPDX-style expressions are checked against an allow list. `OR` needs one allowed option, `AND` needs all, and `WITH` exceptions are ignored. Packages with no licence are reported as `missing`.
- **Bloat:** each package's transitive dependency count, largest first.
- **Command-line tool:** `tpt-dev-dep <Cargo.lock> [licenses.txt] [--allow ID]...` prints the findings. It exits 1 on a cycle or a licence violation, and 2 on a usage error.

## Example

```sh
cargo run -p tpt-dev-dep -- Cargo.lock licenses.txt --allow MIT --allow Apache-2.0
```

`licenses.txt` has one `name license-expression` pair per line. Cargo's lock file does not record licences, so supply them from `cargo metadata` or a `cargo deny` report.

## Limits

- **Lock file subset.** The parser reads `[[package]]` tables with `name`, `version` and `dependencies`. Other fields are ignored, and unusual dependency syntax is an error.
- **Licences are supplied.** The tool does not look them up, so a missing licence is reported rather than checked.
- **No parentheses in expressions.** Expressions with grouping are not parsed, and they count as not allowed.
- **Text output only.** JSON output is not written.
- **Host tool.** It uses `std`, and CI excludes it from the bare-metal build.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

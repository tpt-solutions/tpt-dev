# tpt-dev-cli

`cargo tpt`: one command for the tpt-dev project checks. It runs cargo and the other tpt-dev tools (`tpt-dev-lint`, `tpt-dev-dep`, `tpt-dev-headers`, `tpt-dev-bench`) from this workspace, so the versions always match the checkout.

Install from the workspace (no publish needed):

```sh
cargo install --path crates/tpt-dev-cli
```

Or run it without installing, from the workspace root:

```sh
cargo run -q -p tpt-dev-cli --bin cargo-tpt -- check
```

## Commands

| Command | What it does |
|---|---|
| `cargo tpt check` | `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, the project lint rules over every `.rs` file under `crates/`, and the dependency policy. |
| `cargo tpt check --full` | The same, plus `cargo test --workspace` and `cargo deny check` (skipped with a message if `cargo-deny` is not installed). |
| `cargo tpt lint [--config FILE] [FILE...]` | Project lint rules. With no files, lints every `.rs` file under `crates/`, skipping `target/`, `ui/` and `fixtures/` (trybuild fixtures are deliberately invalid code). |
| `cargo tpt dep [ARGS]` | Reads `cargo metadata`, then checks cycles and licences. Licences come from each package's `license` field. The allow list is read from `deny.toml`, so this check and `cargo deny` agree. Extra arguments go to `tpt-dev-dep`, for example `--include-dev`. |
| `cargo tpt headers SRC GUARD OUT.h` | Generates a C header from Rust `extern "C"` items. |
| `cargo tpt doc` | `cargo doc --workspace --no-deps --all-features` with warnings as errors. |
| `cargo tpt bench-gate [BASELINE] [RESULTS] [-- GATE_ARGS]` | Runs the `bench_suite` example into `RESULTS`, then the gate against `BASELINE`. Defaults: `crates/tpt-dev-bench/examples/ci/baseline.txt` and `target/tpt-bench-results.txt`. Arguments after `--` go to the gate. |

## Exit codes

- `0`: every check passed.
- `1`: a check failed. `check` still runs every step and prints a summary.
- `2`: a usage error, or the command was run outside the workspace.

## How it finds the workspace

It searches upward from the current directory for a `Cargo.toml` that contains `[workspace]`, so it works from any subdirectory.

## Limits

- `check` runs `clippy` and `fmt` again even though the `fmt-lint` CI job runs them too. That keeps the `project-checks` job self-contained.
- The lint step reports the existing `doc-example` and `no-unwrap` warnings. It fails only on `deny` findings, such as a missing `SAFETY` comment.

# tpt-dev-headers

C-header generator that emits Doxygen-ready headers from Rust documentation.

## Overview

When Rust exports a C ABI, the C side needs a header that matches it, and keeping a hand-written header in step with the Rust source is error-prone. `tpt-dev-headers` reads a Rust source file with `syn` and writes a C header for its public C-ABI items. Rust `///` documentation becomes Doxygen `/** ... */` comments.

You can call it as a library (`generate`), from a `build.rs` (`build::write_header`), or as the `tpt-dev-headers` command. It is a host tool: it needs `std` and `syn`, and CI excludes it from the bare-metal build.

It pairs with [`tpt-dev-ffi`](https://docs.rs/tpt-dev-ffi), whose tests generate a header for a C-style library and compile a C client against it in CI.

## Features

- **C-ABI functions:** public `extern "C"` functions become prototypes, with C types for the parameters and the return value.
- **Types:** public `#[repr(C)]` structs and integer-backed enums are emitted with their fields and values. Other public structs become opaque `typedef struct X X;`.
- **Aliases and callbacks:** public type aliases become `typedef`s, including `extern "C" fn` pointer types.
- **Constants:** public integer constants become `#define`s.
- **Doxygen:** `///` documentation becomes `/** ... */` blocks, on items and on struct fields.
- **Include guard:** the guard macro name is set by the caller. The header also has `<stdbool.h>`, `<stddef.h>` and `<stdint.h>` includes and an `extern "C"` block for C++.
- **Change detection:** `write_header` rewrites the output file only when its contents change.

## Installation

```sh
cargo add tpt-dev-headers
```

To install the command from a checkout, use `cargo install --path crates/tpt-dev-headers`.

The crate has no feature flags and is not `no_std`. It is host-only, because it needs `std` and `syn`.

## Quick start

```rust
use tpt_dev_headers::{Options, generate};

let src = r#"
    /// Adds two numbers.
    #[unsafe(no_mangle)]
    pub extern "C" fn add(a: i32, b: i32) -> i32 { a + b }
"#;
let header = generate(src, &Options { guard: "DEMO_H".into() }).unwrap();
assert!(header.contains("int32_t add(int32_t a, int32_t b);"));
assert!(header.contains("/**\n * Adds two numbers."));
```

## Command line

```text
tpt-dev-headers <SOURCE.rs> <GUARD> <OUTPUT.h>
```

Exit codes: `0` the header was written or is already up to date, `1` the header could not be generated or written, `2` the arguments were wrong (including an empty guard). `-h` and `--help` print the usage and exit with `0`.

## Build scripts

`tpt_dev_headers::build::write_header(source, out, guard)` does the same as the command and returns `true` if it wrote the file. `rerun_if_changed` tells Cargo to rerun the build script when the source changes. Add `tpt-dev-headers` under `[build-dependencies]`, then in `build.rs`:

```text
use std::path::Path;
use tpt_dev_headers::build::{rerun_if_changed, write_header};

fn main() {
    let src = Path::new("src/ffi.rs");
    rerun_if_changed(src);
    write_header(src, Path::new("include/ffi.h"), "FFI_H").unwrap();
}
```

Errors are reported as `build::BuildError`, which separates read, generate and write failures.

## Type mapping

| Rust | C |
|---|---|
| `i8`..`i64`, `u8`..`u64` | `int8_t`..`int64_t`, `uint8_t`..`uint64_t` |
| `usize`, `isize` | `size_t`, `ptrdiff_t` |
| `f32`, `f64`, `bool` | `float`, `double`, `bool` |
| `c_int`, `c_uint`, `c_long`, `c_ulong`, `c_char`, `c_float`, `c_double`, `c_void` | `int`, `unsigned int`, `long`, `unsigned long`, `char`, `float`, `double`, `void` |
| `*const T`, `*mut T`, `&T`, `&mut T` | `const T *`, `T *` |
| `extern "C" fn(...)` | function pointer |
| `()` return | `void` |

A named type that is not in this table is written as its own name.

## Design notes and guarantees

- **Deterministic text.** The header is built from the source text alone, so the same input gives the same output. `tests/golden.rs` generates a header for `tests/fixtures/api.rs` and compares it with the frozen `tests/fixtures/api.h`. Run with `BLESS=1` to update the frozen file after reviewing the change.
- **Unchanged files keep their timestamp.** `write_header` compares the new text with the existing file, so C code that includes the header is not rebuilt for nothing.
- **Failures are errors, not guesses.** An unsupported type or construct returns an `Error` that names the item.

## Limitations and scope

- **The C compile check runs only in CI.** The golden file checks the header text. The `c-headers` CI job generates a header from `tpt-dev-ffi`'s test library and compiles `tests/c/client.c` with `gcc -std=c99 -Wall -Werror`. It has not been run locally, because the development machine has no C compiler.
- **Unsupported types are errors.** Arrays, tuples (other than `()`) and generics are rejected.
- **Only some items are read.** Public functions with the `"C"` ABI, structs, enums, type aliases and integer constants are emitted. Other items are ignored.
- **`#[repr(C)]` structs need named fields.** A non-`repr(C)` struct becomes an opaque typedef.
- **Enums use prefixed names.** Variants are emitted as `Enum_Variant`, and discriminants must be integer literals.
- **Constants must be integer literals.** An optional leading minus sign is accepted.
- **Parameter names of function-pointer types are dropped.**
- **Host tool.** It uses `syn` and `std`.

## Related crates

- [`tpt-dev-ffi`](https://docs.rs/tpt-dev-ffi): typestate sessions for C-style APIs. It uses this crate as a dev-dependency to check a generated header.

This crate depends on no other workspace crate.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

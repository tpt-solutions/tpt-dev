# tpt-dev-headers

A C header generator. It reads a Rust source file and writes a C header for its C-ABI surface, with Doxygen comments taken from the Rust docs.

- **C-ABI functions:** public `extern "C"` functions become prototypes, with C types for the parameters and the return value.
- **Types:** `#[repr(C)]` structs and integer-backed enums are emitted with their fields and values. Other public structs become opaque `typedef struct X X;`.
- **Aliases and callbacks:** public type aliases become `typedef`s, including `extern "C" fn` pointer types.
- **Constants:** public integer constants become `#define`s.
- **Doxygen:** `///` documentation becomes `/** ... */` blocks, on items and on struct fields.
- **Include guard:** set by the caller.

## Example

```rust
use tpt_dev_headers::{Options, generate};

let src = r#"
    /// Adds two numbers.
    #[unsafe(no_mangle)]
    pub extern "C" fn add(a: i32, b: i32) -> i32 { a + b }
"#;
let header = generate(src, &Options { guard: "DEMO_H".into() }).unwrap();
```

`tests/golden.rs` generates a header for `tests/fixtures/api.rs` and compares it with the frozen `api.h`. Run with `BLESS=1` to update the frozen file after reviewing the change.

## Type mapping

| Rust | C |
|---|---|
| `i8`..`i64`, `u8`..`u64` | `int8_t`..`int64_t`, `uint8_t`..`uint64_t` |
| `usize`, `isize` | `size_t`, `ptrdiff_t` |
| `f32`, `f64`, `bool` | `float`, `double`, `bool` |
| `c_int`, `c_char`, ... | `int`, `char`, ... |
| `*const T`, `*mut T` | `const T *`, `T *` |
| `extern "C" fn(...)` | function pointer |
| `()` return | `void` |

## Limits

- **Not compiled in CI yet.** The generated headers are checked against a frozen golden file. The check that they compile under a C compiler is not wired up, because the environment has no C compiler.
- **Unsupported types are errors.** Arrays, tuples and generics are rejected rather than guessed.
- **Enums use prefixed names.** Variants are emitted as `Enum_Variant`, and discriminants must be integer literals.
- **Parameter names of function-pointer types are dropped.**
- **Host tool.** It uses `syn` and `std`, and CI excludes it from the bare-metal build.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

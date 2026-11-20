# tpt-dev-mock-macros

The `#[mock]` attribute for `tpt-dev-mock`.

## Overview

This crate holds the proc-macro behind [`tpt-dev-mock`](https://docs.rs/tpt-dev-mock). You normally do not depend on it directly: use `tpt_dev_mock::mock`, which re-exports the attribute. The generated code refers to `::tpt_dev_mock::Recorder`, so the macro only works in a crate that depends on `tpt-dev-mock`.

It is a proc-macro crate, so it runs on the host at compile time. It depends only on `syn`, `quote` and `proc-macro2`.

## Features

- **`#[mock]`:** for `trait T`, generates `MockT` with one `pub` recorder field per method, a `new` and a `verify` method, a `Default` impl, and `impl T for MockT`.
- **Trait is preserved:** the original trait is emitted unchanged, and the mock takes the trait's visibility.
- **Clear errors:** unsupported shapes are rejected with a compile error that points at the offending item.

## Installation

```sh
cargo add tpt-dev-mock
```

Add `tpt-dev-mock` rather than this crate. There are no feature flags. The crate is host-only (`proc-macro = true`).

## Quick start

```rust
use tpt_dev_mock::mock;

#[mock]
pub trait Sensor {
    fn read(&self, channel: u8) -> u16;
}

let mock = MockSensor::new();
mock.read.expect().returns(7);
assert_eq!(mock.read(1), 7);
```

This crate's README is not compiled here, because the example needs the runtime crate. The same pattern is compiled and run as a doctest in `tpt-dev-mock` and in its `tests/mock.rs`.

## Design notes and guarantees

- **Argument tuples:** each method's arguments are passed to its recorder as a tuple, named `arg0`, `arg1` and so on in the generated impl.
- **No arguments to the attribute:** `#[mock(...)]` is an error.
- **Errors keep the trait:** on failure the trait is emitted next to the error, so unrelated code still compiles.

## Limitations and scope

- **Rejected:** generic traits, generic methods, async methods, associated types and constants, associated functions without a receiver, and by-value `self`.
- **Owned types:** argument and return types must be `'static`, because the recorder stores boxed closures.
- **Failures are tested:** the `tpt-dev-mock` crate has compile-fail tests for generic methods and missing receivers.

## Related crates

- [`tpt-dev-mock`](https://docs.rs/tpt-dev-mock): the runtime and the public entry point.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

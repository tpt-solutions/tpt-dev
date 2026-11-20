# tpt-dev-mock

Compile-time, zero-boilerplate mocking for traits.

## Overview

`tpt-dev-mock` generates a mock for a trait from one attribute. You write `#[mock]` on the trait, and the macro emits a `Mock<Trait>` struct that implements it. Each method of the mock has a recorder that holds expectations, matches calls against them, and counts calls. A call that no expectation answers panics, so the test fails.

It is for tests of code that depends on a trait object or a generic bound, in `std` and `no_std` projects. The runtime needs `alloc` (expectations are boxed) and has no dependency beyond the macro crate. The `#[mock]` attribute lives in [`tpt-dev-mock-macros`](https://docs.rs/tpt-dev-mock-macros) and is re-exported here.

## Features

- **One attribute:** `#[mock]` on a trait generates `MockTrait`, which implements the trait and keeps the trait itself unchanged.
- **Expectations:** `mock.method.expect().with(matcher).times(n).returns(value)`, or `returns_with(closure)` to compute the answer from the arguments.
- **Call counts and verification:** `calls()` reports how many times a method was called, and `verify()` panics if a `times` expectation was not met.
- **Unexpected calls panic:** a call that no expectation answers fails the test with `unexpected call to `name``.
- **`&self` and `&mut self` methods:** both are supported.
- **Associated types:** bind each one in the attribute, as in `#[mock(Item = u32)]`. `Self::Item` in method signatures becomes the bound type.
- **Compile errors for unsupported shapes:** generic traits, generic or async methods, associated constants, unbound associated types and methods without a `&self` or `&mut self` receiver are rejected with a message.

## Installation

```sh
cargo add tpt-dev-mock
```

| Feature | Enables | Default |
| ------- | ------- | ------- |
| `std`   | Links `std`. The crate itself uses only `core` and `alloc`. | off |

The crate is `#![no_std]` and requires `alloc`. The macro crate is a proc-macro and runs on the host at compile time only.

## Quick start

```rust
use tpt_dev_mock::mock;

#[mock]
pub trait Sensor {
    fn read(&self, channel: u8) -> u16;
}

fn total(sensor: &dyn Sensor) -> u32 {
    u32::from(sensor.read(1)) + u32::from(sensor.read(2))
}

let mock = MockSensor::new();
mock.read.expect().with(|a| a.0 == 1).times(1).returns(100);
mock.read.expect().with(|a| a.0 == 2).times(1).returns(23);
assert_eq!(total(&mock), 123);
mock.verify();
```

Arguments reach a matcher or `returns_with` closure as a tuple, `()` for none. Methods without a return type return `()`, so use `returns(())` for them.

```rust
use tpt_dev_mock::mock;

#[mock]
pub trait Counter {
    fn add(&mut self, by: u32) -> u32;
}

let mut mock = MockCounter::new();
mock.add.expect().returns_with(|(by,)| by * 2);
assert_eq!(mock.add(21), 42);
assert_eq!(mock.add.calls(), 1);
```

## Design notes and guarantees

- **Order of matching:** expectations are checked in the order they were added. The first one whose matcher accepts the arguments, and that has calls left, answers the call.
- **Default matcher:** an expectation with no `with` matches every call.
- **Counts:** `times(n)` caps an expectation at `n` calls and `verify()` requires exactly `n`. Without `times`, an expectation answers any number of calls and `verify()` does not check it.
- **Nothing is recorded early:** an expectation takes effect when `returns` or `returns_with` is called.
- **Interior mutability:** recorders use `RefCell` and `Cell`, so a mock is not `Sync`. It is meant for single-threaded tests.
- **No panics in non-test paths:** the only panics are the intended test failures from `call` and `verify`.

## Limitations and scope

- **Owned arguments and results.** Argument and return types must be `'static`, so references in signatures are not supported.
- **No generic traits or methods, no async methods.** These are rejected with a compile error.
- **Associated types must be bound.** Each `type Name;` needs `#[mock(Name = Type)]`. Generic associated types are rejected. Only `Self::Name` paths are rewritten, so the qualified form `<Self as Trait>::Name` is not.
- **No associated constants.** Traits that have them cannot be mocked yet.
- **Receivers:** methods must take `&self` or `&mut self`. By-value `self` and associated functions are rejected.
- **`returns` needs `Clone`.** Use `returns_with` for results that are not `Clone`.
- **Matchers see arguments, not history.** A matcher is a closure over one call's arguments.
- **Compile time is measured on one machine.** See the benchmark below; the numbers are ratios from one host, not a guarantee.

## Compile-time benchmark

`examples/compile_bench.rs` compares `#[mock]` with mockall's `#[automock]`. For each trait size it generates two scratch crates outside the repository. Each crate has one trait with N methods taking `(u8, u16)` and returning `u32`, plus one test per method that sets an expectation, makes the call and verifies it. Run it from the repository root:

```text
cargo run --release -p tpt-dev-mock --example compile_bench -- <scratch-dir> 10,50 9
```

- **cold:** the first `cargo test --no-run`, which also compiles dependencies. For mockall this includes mockall and its dependencies.
- **median and min:** rebuild times after a source edit, which recompile only the scratch crate with dependencies warm. The two libraries are rebuilt alternately, so both see the same load. `vs tpt` is the median relative to `tpt-dev-mock`.

Results from one run on Windows 11, release profile, 9 rebuilds per library (other applications were running):

| methods | library | cold (s) | median (s) | min (s) | vs tpt |
|---:|---|---:|---:|---:|---:|
| 10 | tpt-dev-mock | 8.30 | 1.27 | 1.07 | 1.00x |
| 10 | mockall | 9.30 | 1.38 | 1.34 | 1.09x |
| 50 | tpt-dev-mock | 9.15 | 1.12 | 0.95 | 1.00x |
| 50 | mockall | 12.46 | 2.16 | 1.84 | 1.94x |

Earlier runs of the same benchmark, without interleaving, were less stable. The 10-method gap is within the spread between runs. The 50-method gap is larger but was measured on one machine, so treat it as indicative.

## Related crates

- [`tpt-dev-mock-macros`](https://docs.rs/tpt-dev-mock-macros): the `#[mock]` proc-macro, a dependency of this crate.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

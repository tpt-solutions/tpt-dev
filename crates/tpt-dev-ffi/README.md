# tpt-dev-ffi

Zero-copy FFI state machine framework for stateful C API lifetimes.

## Overview

Many C libraries expose an object with a lifecycle: create, open, start, stop, close, destroy. The rules about which call is allowed in which state live in the documentation, and a wrong call fails at run time, or corrupts state. `tpt-dev-ffi` puts each lifecycle state in a separate Rust type. An operation that is not allowed in the current state does not compile, and the library object is destroyed exactly once, on every path.

You describe the library once, by implementing the `Backend` trait. The `Session` type then supplies the typestate transitions and the release-on-drop logic, without depending on the library. The crate is `no_std`, has no runtime dependencies, and does not allocate.

The crate is meant for code that wraps a C-style API. Its tests use `tpt-dev-automata` to check that the typestates match a state machine, and `tpt-dev-headers` to check the C header generated for the library's exported functions. Both are dev-dependencies only.

## Features

- **Typestate sessions:** `Closed`, `Open` and `Streaming`. Each operation consumes the session and returns it in its new state.
- **Failures keep the session:** a failed operation returns `Failed`, which holds the session unchanged and the library's `Code`. The caller decides what to do next.
- **Release on drop:** dropping a session calls `Backend::destroy` once, whatever state the session is in.
- **Checked handles:** `Handle<T>` is a non-null, typed pointer to a library object. It cannot be built from null.
- **Backend trait:** the library's operations are described once, in a `Backend` implementation.
- **Zero-copy views:** `Streaming` sessions have `with_view`, which gives a closure a read-only `View` of the library's bytes with no copy. The view cannot be stored or returned, and no library call can run while it is alive.

## Installation

```sh
cargo add tpt-dev-ffi
```

The crate is `#![no_std]` and does not need `alloc`.

| Feature | Enables | Default |
|---|---|---|
| `std` | Links `std`. It adds no public API. | off |

## Quick start

```rust
use tpt_dev_ffi::{Backend, Code, Session};

struct Fake { log: Vec<&'static str> }

impl Backend for Fake {
    type Handle = u32;
    fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    fn destroy(&mut self, _: u32) { self.log.push("destroyed"); }
}

let mut fake = Fake { log: Vec::new() };
let session = Session::new(&mut fake).create().unwrap();
let streaming = session.open().unwrap().start().unwrap();
let open = streaming.stop().unwrap();
drop(open); // frees the library object
assert_eq!(fake.log, ["destroyed"]);
```

Calling `start` on a `Closed` session does not compile. `tests/ui/start_closed.rs` checks that with `trybuild`.

For a real library, `tests/c_library.rs` defines a stateful library with `extern "C"` functions, wraps it in a `Backend`, and drives it through the typestates.

## Design notes and guarantees

1. The session owns the library object. It is destroyed exactly once, when the session is dropped, in whatever state it is in.
2. Transitions take the session by value. `Session::with` and `Session::with_view` borrow the session for the call, so a handle does not outlive the session that produced it.
3. Buffers passed to the library are borrowed for one call, through `with`. Nothing about them is kept after the call.
4. A `View` exists only inside the closure given to `with_view`. The closure takes the view for any lifetime (`for<'v>`), so the view cannot escape. The session is borrowed mutably for the whole call, so no library call runs while the view is alive. The cases in `tests/ui/view_*.rs` check these rules at compile time.
5. `Backend::view` returns a slice tied to the backend borrow. The implementer must keep the bytes valid and unchanged for that borrow. The default implementation returns `None`.
6. A transition on a session that has no library object (before `create` succeeds) returns `Failed` with `Code(-1)` instead of calling the backend.

The crate sets `#![allow(unsafe_code)]` in `lib.rs`, but the code it ships currently contains no `unsafe` blocks. The `unsafe` code in the tests belongs to the example C-style library.

## Limitations and scope

- **Only the typestate is checked by the compiler.** The library's own rules are checked at run time, through the codes it returns.
- **One backend per session.** A session borrows its backend mutably, so two sessions cannot share one.
- **Views are read-only and scoped.** A `View` covers only the bytes that `Backend::view` exposes, and `with_view` exists only in the `Streaming` state.
- **`destroy` cannot fail.** `create` reports its code, but cleanup errors from `destroy` are not reported.
- **C is not compiled here.** The example library is Rust, exported through `extern "C"`. The C compile check is a CI job in the `tpt-dev-headers` setup.
- **Hand-written machine.** The `tpt_dev_automata::Machine` used in `tests/c_library.rs` is written by hand, not generated from the typestates. The two can drift apart if one changes alone. The test checks that the machine is deadlock-free, that all states are reachable, and that every typestate transition is a machine transition.

## Related crates

- [`tpt-dev-headers`](https://docs.rs/tpt-dev-headers): generates the C header for a library's exported functions. Dev-dependency, used in `tests/c_library.rs`.
- [`tpt-dev-automata`](https://docs.rs/tpt-dev-automata): state machines. Dev-dependency, used to check the typestates.

No other workspace crate depends on `tpt-dev-ffi`.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

# tpt-dev-ffi

A framework for stateful C-style APIs. Each lifecycle state is a separate type, so an operation that is not allowed in the current state does not compile. Ownership of the library object is tied to the session, so it is released on every path.

- **Typestate sessions:** `Closed`, `Open` and `Streaming`. Each operation consumes the session and returns it in its new state.
- **Failures keep the session:** a failed operation returns the session unchanged, with the library's code. The caller decides what to do next.
- **Release on drop:** dropping a session frees the library object, whatever state it is in.
- **Checked handles:** `Handle<T>` is a non-null, typed pointer to a library object. It cannot be built from null.
- **Backend trait:** the library's operations are described once, in a `Backend` implementation. The session logic does not depend on the library.

## Example

The `tests/c_library.rs` test defines a stateful library with `extern "C"` functions, wraps it in a `Backend`, and drives it through the typestates:

```rust,ignore
let mut backend = CBackend::default();
let session = Session::new(&mut backend).create().unwrap();
let streaming = session.open().unwrap().start().unwrap();
let open = streaming.stop().unwrap();
drop(open); // frees the library object
```

A `start` on a closed session does not compile. `tests/ui/start_closed.rs` checks that.

## Limits

- **Only the typestate is checked by the compiler.** The library's own rules are checked at run time, through the codes it returns.
- **One backend per session.** A session borrows its backend mutably, so two sessions cannot share one.
- **No borrowed data in the handle.** Buffers passed to C must be borrowed around the call, with `with`. Zero-copy views that outlive a call are not modelled.
- **Errors from `create` and `destroy`.** `create` reports its code, but `destroy` cannot fail, so cleanup errors are not reported.
- **C not compiled here.** The example library is Rust, exported through `extern "C"`, because the environment has no C compiler.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

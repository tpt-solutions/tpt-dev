# tpt-dev-compose

Parses core WebAssembly modules in pure Rust, and links them by their imports and exports. It checks that each function import has a matching export with the same signature, and orders the modules so that providers come before the modules that use them.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

- **Parser:** `parse` reads the module's types, imports, function signatures, exports and start function. It checks LEB128 encodings, section order, section lengths, and that every type, function and code count agrees. Table, memory, global, element and data sections are length-checked and skipped.
- **Linker:** `Registry` holds named modules. `link` resolves every function import, checks signatures, and returns a `Plan`. The plan has the instantiation order and every binding. Cycles are reported.
- **Conformance:** the fixtures in `tests/fixtures` are written in WAT and assembled with `wasm-tools`. `wasm-tools validate` accepts each one. The parser's view of the client module matches `wasm-tools print`.

## Example

```rust
use tpt_dev_compose::{Registry, parse};

// An empty module: it has no imports and no exports.
let provider = parse(&[0, b'a', b's', b'm', 1, 0, 0, 0]).unwrap();
let mut registry = Registry::new();
registry.register("provider", &provider).unwrap();
let plan = registry.link().unwrap();
assert_eq!(plan.order, [0]);
assert!(plan.bindings.is_empty());
```

## Not done yet

- **Component Model.** The type system and the canonical ABI are not written. This crate reads core modules only.
- **Runtime composition.** The linker plans. It does not instantiate modules or run them, and there is no interpreter.
- **Other imports.** Imported tables, memories, globals and tags are not resolved. Linking refuses them with `UnsupportedImport`.

## Limits

- **Code is not validated.** Function bodies are length-checked and skipped. Instructions are not decoded, so a module with invalid code can parse.
- **Some indices are not checked.** Table, memory, global and tag indices are not bounds-checked. Function indices in exports and the start function are.
- **Memory limits are 32-bit.** Only the 32-bit limits forms are read. A memory64 limits flag is refused.

## Safety review

The parser reads untrusted bytes, so each step was checked for resource use:

- **Counts are bounded by the input.** Every element takes at least one byte, so a count larger than the bytes left is refused before anything is allocated.
- **LEB128 cannot overflow.** A `u32` is refused if it is longer than five bytes, or if the fifth byte has bits beyond the low four.
- **Sections stay in bounds.** A section that claims more bytes than remain is refused. A section with bytes left over after its contents is refused.
- **No recursion.** Types and sections are read in a single pass, so input nesting cannot exhaust the stack.
- **Names are checked.** Import and export names must be valid UTF-8.
- **The linker is not a sandbox.** Linking decides which module calls which. It grants no permissions, and the code of the linked modules still runs under whatever runtime executes it.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

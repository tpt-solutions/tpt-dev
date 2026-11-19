# tpt-dev-compose

Parses core WebAssembly modules and components in pure Rust. It links core modules by their imports and exports, checking the type of every import (functions, tables, memories, globals and tags), and orders the modules so that providers come before the modules that use them. It also decodes component value and function types, and computes their canonical ABI layouts.

**Linking plans and does not execute.** `Registry::link` returns a `Plan`: the instantiation order and the export each import is bound to. Nothing is instantiated or run. Running a plan needs a WebAssembly engine, and this crate does not provide one.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

- **Parser:** `parse` reads the module's types, imports, function signatures, tables, memories, globals, exports and start function. It checks LEB128 encodings, section order, section lengths, and that every type, function and code count agrees. Every index is bounds-checked against its index space, including imported entries, and export names must be unique. Limits and constant expressions are checked. Element and data sections are length-checked and skipped.
- **Component parser:** `component::parse` reads a component's sections. For each one it gives the id, offset, size, and the item count of vector sections. It returns the names of custom sections, the embedded core modules (parsed with the module parser), and nested components, up to a depth of 16. Type sections are decoded (see `types`). Other vector sections are not decoded, so component imports and exports are not available. Checked against `wasm-tools dump` for four fixtures, one of which contains a nested component.
- **Component types:** `types` decodes the defined value types (primitives, records, variants, lists, tuples, flags, enums, options, results, handles) and function types. Instance, component, resource and module types are not decoded: a type section that holds one has `types: None`. `layout` and `flatten` compute the canonical ABI size, alignment and flat core types of a value type, given the type index space. Their values are checked against hand-worked cases. The encodings are checked against `wasm-tools`, which does not report layouts.
- **Linker:** `Registry` holds named modules. `link` resolves every import of every core kind, checks each against its export's type, and returns a `Plan`. The plan has the instantiation order and every binding, with its kind. Cycles are reported.
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

- **Component-level imports and exports.** Component import, export, alias, canonical function and instance sections are not decoded. Component-level import kinds (value, type, instance, component) are not resolved, and the linker handles core modules only.
- **Instance, component and resource types.** These are not decoded (see above). Streams, futures, error contexts and fixed-size lists are not decoded either.
- **Execution is not implemented.** The linker plans and does not instantiate or run modules, and there is no interpreter. This is out of scope for this crate's current pass.

## Limits

- **Code is not validated.** Function bodies are length-checked and skipped. Instructions are not decoded, so a module with invalid code can parse. Constant expressions are checked for their opcodes only, not their types.
- **Layouts are bounded.** `layout` and `flatten` stop with `TooComplex` past 64 levels of nesting or 100,000 steps, so a type that refers to itself or doubles at each level cannot exhaust the stack or the time.
- **Memory limits are 32-bit.** Only the 32-bit limits forms are read. A memory64 limits flag is refused.

## Safety review

The parser reads untrusted bytes, so each step was checked for resource use:

- **Counts are bounded by the input.** Every element takes at least one byte, so a count larger than the bytes left is refused before anything is allocated.
- **LEB128 cannot overflow.** A `u32` is refused if it is longer than five bytes, or if the fifth byte has bits beyond the low four.
- **Sections stay in bounds.** A section that claims more bytes than remain is refused. A section with bytes left over after its contents is refused.
- **Bounded recursion.** Core modules are read in one pass. Nested components recurse, and the depth is limited to 16, so input nesting cannot exhaust the stack.
- **Names are checked.** Import and export names must be valid UTF-8.
- **The linker is not a sandbox.** Linking decides which module calls which. It grants no permissions, and the code of the linked modules still runs under whatever runtime executes it.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

# tpt-dev-compose

Parses core WebAssembly modules and components in pure Rust. It links core modules by their imports and exports, checking the type of every import (functions, tables, memories, globals and tags), and orders the modules so that providers come before the modules that use them. It also decodes component value and function types, and computes their canonical ABI layouts.

**Linking plans and does not execute.** `Registry::link` returns a `Plan`: the instantiation order and the export each import is bound to. Nothing is instantiated or run. Running a plan needs a WebAssembly engine, and this crate does not provide one.

**Status:** 0.0.0, not published. Part of the [tpt-dev](https://github.com/tpt-solutions/tpt-dev) workspace.

- **Parser:** `parse` reads the module's types, imports, function signatures, tables, memories, globals, exports and start function. It checks LEB128 encodings, section order, section lengths, and that every type, function and code count agrees. Every index is bounds-checked against its index space, including imported entries, and export names must be unique. Limits and constant expressions are checked. Element and data sections are length-checked and skipped.
- **Component parser:** `component::parse` reads a component's sections. For each one it gives the id, offset, size, and the item count of vector sections. It returns the names of custom sections, the embedded core modules (parsed with the module parser), and nested components, up to a depth of 16. Type, alias, import, canonical and export sections are decoded (see below). Canonical entries are `canon lift`, `canon lower` and the three resource built-ins. Other built-ins, and the `async`, `callback`, `core-type` and `gc` options, are refused with a typed error. Index bounds are exact in every space, checked in section order: a reference must name an entry defined before it. Outer aliases are checked against the scope they name. The memory option of a canonical entry is not checked, since core memories are not tracked. Core exports other than modules are refused. Checked against `wasm-tools dump` for the section layout of the fixtures, including one with a nested component.
- **Component types:** `types` decodes the defined value types (primitives including `error-context`, records, variants, lists, tuples, flags, enums, options, results, handles, streams, futures, maps and fixed-length lists), resource types, function types (sync and async), and component, instance and core module types with their declarations (core function types, core module types with their imports, exports, types and outer aliases, type definitions, aliases, imports and exports, including nested component types). A type export defines a type, as the component model says. Only bare core function types and core module types are decoded: a rec group or GC type makes the section `types: None`, and so does any type this parser does not decode. Malformed bytes are a `ParseError`. `layout` and `flatten` compute the canonical ABI size, alignment and flat core types of a value type. Streams, futures and error contexts are `i32` handles. A map has the layout of a list of pairs. A fixed-length list has its element's size and alignment times its length, and an empty one is refused. `func_abi` gives how a function's parameters and results cross the core boundary: when they are spilled to memory (more than 16 flat parameters, or more than one flat result), the core signature that results, and the memory tuple with each value's offset. `tuple_layout` gives the layout of a tuple. Their values are checked against hand-worked cases, and the spill against `wasm-tools validate`, which rejects the non-spilled signature. The encodings are checked against `wasm-tools`, which does not report layouts.
- **Linker:** `Registry` holds named modules and components. `link` resolves every import of every core kind, checks each against its export's type, and returns a `Plan`. Component imports are matched by name and kind against the exports of registered components, and must match exactly one. Their types are compared structurally (see `compare`): functions, values, types, and instance types, where a provider instance may export more than the importer asks for. A comparison that reaches an alias, an import of a type, a resource or a component type is not refused. The plan has the instantiation order and every binding for both, with its kind. Cycles are reported.
- **Conformance:** the fixtures in `tests/fixtures` are written in WAT and assembled with `wasm-tools`. `wasm-tools validate` accepts each one. The parser's view of the client module matches `wasm-tools print`. The fixtures in `tests/fixtures/constructs` cover canonical functions, value kinds, core module types and outer aliases. They need `wasm-tools validate --features all`, since fixed-length lists and async types are behind that feature. Their decoded values match `wasm-tools dump`, and every prefix and single-byte corruption of them is parsed without a panic.

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

- **Canonical built-ins and instances.** The canonical built-ins other than lift, lower and the resource intrinsics (task, stream, future, thread, waitable and context intrinsics, and the async, callback, core-type and gc options) are refused. The instance section (5) is counted, not decoded, so a reference into the instance space is only bounded, and a component import whose type depends on an instantiation is not type-checked. Component-level composition is planned, not instantiated.
- **Outer aliases and comparisons.** Outer aliases are bounds-checked against the scopes they name. Comparisons do not decide types that have no comparison form yet (async functions, maps, fixed-length lists, streams, futures), so they give no answer for them.
- **Top-level core types.** Entries of the top-level core type section (3) are counted, not decoded. Only core types declared inside a component or instance type are decoded.
- **Resources.** Resources are not compared across components, since a resource's identity comes from the instance that defines it.
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

# tpt-dev-codec

Compile-time validated, zero-allocation serialization.

## Overview

`tpt-dev-codec` serializes fixed-size binary layouts, such as packed register maps and wire frames. A layout is a `const` list of named fields, each with a bit offset, a width and a byte order. The layout is checked when it is built, so a bad schema fails the build. `encode` and `decode` work on caller-provided slices and never allocate.

It is for embedded and `no_std` code that exchanges small, fixed-format messages. The crate is `#![no_std]` and needs no `alloc`. It has no dependency on other `tpt-dev-*` crates (`tpt-dev-test` is used only in its tests).

## Features

- **Compile-time validation:** a layout is built in a `const`. An overlap, a field past the end, a width outside 1 to 64 bits, a duplicate or empty name, or an unaligned big-endian field is a build error.
- **Schema evolution:** `Layout::extends` checks that a newer version keeps every field of the old one, in the same place. A `const` assertion turns an incompatible change into a build error.
- **Bit fields and endianness:** little-endian fields pack from the least significant bit, and big-endian fields are whole, aligned bytes.
- **Zero-allocation:** `encode` and `decode` work on caller-provided slices, and bits no field covers are written as zero.
- **Errors, not panics, at run time:** a short buffer, a wrong value count, or a value too wide for its field returns a `CodecError`.

## Installation

```sh
cargo add tpt-dev-codec
```

| Feature | Enables | Default |
| ------- | ------- | ------- |
| `std`   | Links `std`. The crate itself uses only `core`, so this changes nothing in the API. | off |

The crate is `#![no_std]` and does not need `alloc`.

## Quick start

```rust
use tpt_dev_codec::{Field, Layout};

const FIELDS: [Field; 3] = [
    Field::bits("flags", 0, 4),
    Field::bits("len", 4, 12),
    Field::be("sensor", 16, 16),
];
const LAYOUT: Layout<4> = Layout::new(&FIELDS);

let mut buf = [0u8; 4];
LAYOUT.encode(&[0b1010, 0x123, 0xBEEF], &mut buf).unwrap();
let mut out = [0u64; 3];
LAYOUT.decode(&buf, &mut out).unwrap();
assert_eq!(out, [0b1010, 0x123, 0xBEEF]);
```

Values are passed one per field, in declaration order, as `u64`. `Layout<B>` is a layout of `B` bytes.

### Errors

```rust
use tpt_dev_codec::{CodecError, Field, Layout};

const FIELDS: [Field; 1] = [Field::bits("nibble", 0, 4)];
const LAYOUT: Layout<1> = Layout::new(&FIELDS);

let mut buf = [0u8; 1];
assert_eq!(
    LAYOUT.encode(&[0x10], &mut buf),
    Err(CodecError::TooWide { field: 0, width: 4 })
);
assert_eq!(
    LAYOUT.encode(&[1], &mut []),
    Err(CodecError::BufferTooSmall { needed: 1 })
);
```

### Schema evolution

```rust
use tpt_dev_codec::{Field, Layout};

const V1_FIELDS: [Field; 1] = [Field::bits("id", 0, 8)];
const V2_FIELDS: [Field; 2] = [Field::bits("id", 0, 8), Field::bits("flags", 8, 8)];
const V1: Layout<2> = Layout::new(&V1_FIELDS);
const V2: Layout<2> = Layout::new(&V2_FIELDS).with_version(2);
const _: () = assert!(V2.extends(&V1));
```

`extends` is true when the version is higher and every field of the base is still present with the same name, offset, width and endianness. New fields may be added.

## Design notes and guarantees

- **Build-time checks:** `Layout::new` is a `const fn` that panics on an invalid field list. In a `const` item, that panic is a compile error. The tests include compile-fail cases for overlap, unaligned big-endian fields and an incompatible schema change.
- **No allocation:** encode and decode touch only the slices passed in. `encode` writes the first `B` bytes and zeroes bits that no field covers.
- **Round trips are property-tested:** the tests in `tests/roundtrip.rs` check random values against a layout, using `tpt-dev-test`.
- **No `unsafe` code.**

## Limitations and scope

- **No derive macro.** A schema is written as a `const` list of fields. A derive that builds the list from a Rust struct is not written.
- **Fixed size.** A layout has a fixed number of bytes, set by its type parameter. Variable-length data, such as strings or nested messages, is not supported.
- **Up to 64 fields, 64-bit values.** A layout holds at most 64 fields, and each field is 1 to 64 bits wide.
- **Big-endian needs alignment.** A big-endian field must start on a byte boundary and span whole bytes.
- **Bit-by-bit codec.** Little-endian fields are read and written one bit at a time. The crate makes no throughput claim.
- **No coverage-guided fuzzing.** The round-trip tests use random inputs, and there is no fuzz target yet.

## Related crates

This crate has no workspace dependencies at run time, and no workspace crate depends on it. [`tpt-dev-test`](https://docs.rs/tpt-dev-test) is a dev-dependency used for the property tests.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.

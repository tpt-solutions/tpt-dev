# tpt-dev-codec

Serialization for fixed-size binary layouts. The layout is checked when it is built, and encode and decode write into caller buffers, so they never allocate.

- **Compile-time validation:** a layout is built in a `const`. An overlap, a field past the end, a width outside 1 to 64 bits, a duplicate name, or an unaligned big-endian field is a build error.
- **Schema evolution:** `Layout::extends` checks that a newer version keeps every field of the old one, in the same place. A `const` assertion turns an incompatible change into a build error.
- **Bit fields and endianness:** little-endian fields pack from the least significant bit, and big-endian fields are whole, aligned bytes.
- **Zero-allocation:** `encode` and `decode` work on caller-provided slices, and bits no field covers are written as zero.

## Example

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

## Schema evolution

```rust
use tpt_dev_codec::{Field, Layout};

const V1_FIELDS: [Field; 1] = [Field::bits("id", 0, 8)];
const V2_FIELDS: [Field; 2] = [Field::bits("id", 0, 8), Field::bits("flags", 8, 8)];
const V1: Layout<2> = Layout::new(&V1_FIELDS);
const V2: Layout<2> = Layout::new(&V2_FIELDS).with_version(2);
const _: () = assert!(V2.extends(&V1));
```

## Limits

- **No derive macro.** A schema is written as a `const` list of fields. A derive that builds the list from a Rust struct is not written.
- **Fixed size.** A layout has a fixed number of bytes, set by its type parameter. Variable-length data, such as strings or nested messages, is not supported.
- **Up to 64 fields.** A layout holds at most 64 fields and 64-bit values.
- **Big-endian needs alignment.** A big-endian field must start on a byte boundary and span whole bytes.
- **Round trips are property-tested.** The tests check random values against the layout. There is no coverage-guided fuzz target yet.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.

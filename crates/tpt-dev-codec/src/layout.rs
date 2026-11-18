//! A validated layout of fields in a fixed number of bytes, and its encoder and decoder.

use core::fmt;

use crate::field::{Endian, Field, str_eq, validate};

/// Why an encode or decode failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecError {
    /// The output buffer (encode) or input buffer (decode) is shorter than the layout.
    BufferTooSmall {
        /// Bytes needed.
        needed: usize,
    },
    /// The number of values does not match the number of fields.
    WrongValueCount {
        /// Fields in the layout.
        expected: usize,
        /// Values given.
        got: usize,
    },
    /// A value does not fit in its field's width.
    TooWide {
        /// Index of the field in the layout.
        field: usize,
        /// The field's width in bits.
        width: u8,
    },
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::BufferTooSmall { needed } => write!(f, "buffer needs {needed} bytes"),
            CodecError::WrongValueCount { expected, got } => {
                write!(f, "expected {expected} values, got {got}")
            }
            CodecError::TooWide { field, width } => {
                write!(f, "value for field {field} does not fit in {width} bits")
            }
        }
    }
}

/// A set of fields in `B` bytes, checked when it is built.
///
/// Build a layout in a `const` item, so that the checks run at compile time.
/// An overlap, a field past the end, a bad width, or a big-endian field that
/// is not byte-aligned is then a build error.
///
/// ```
/// use tpt_dev_codec::{Field, Layout};
///
/// const FIELDS: [Field; 3] = [
///     Field::bits("flags", 0, 4),
///     Field::bits("len", 4, 12),
///     Field::be("sensor", 16, 16),
/// ];
/// const LAYOUT: Layout<4> = Layout::new(&FIELDS);
///
/// let mut buf = [0u8; 4];
/// LAYOUT.encode(&[0b1010, 0x123, 0xBEEF], &mut buf).unwrap();
/// let mut out = [0u64; 3];
/// LAYOUT.decode(&buf, &mut out).unwrap();
/// assert_eq!(out, [0b1010, 0x123, 0xBEEF]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout<const B: usize> {
    fields: &'static [Field],
    version: u16,
}

impl<const B: usize> Layout<B> {
    /// Builds a version-1 layout. Panics at compile time if the fields are invalid.
    pub const fn new(fields: &'static [Field]) -> Self {
        validate(fields, B);
        Self { fields, version: 1 }
    }

    /// Sets the schema version. Use it with [`Layout::extends`] to check evolution.
    pub const fn with_version(self, version: u16) -> Self {
        Self { version, ..self }
    }

    /// The fields, in declaration order.
    pub const fn fields(&self) -> &'static [Field] {
        self.fields
    }

    /// The schema version.
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Size of the layout in bytes.
    pub const fn bytes(&self) -> usize {
        B
    }

    /// Returns `true` when this layout is a compatible newer version of `base`.
    ///
    /// Compatible means the version is higher, and every field of `base` is
    /// still present with the same name, offset, width and endianness. New
    /// fields may be added. Use it in a const assertion, so that an
    /// incompatible change to a schema fails the build:
    ///
    /// ```
    /// use tpt_dev_codec::{Field, Layout};
    ///
    /// const V1_FIELDS: [Field; 1] = [Field::bits("id", 0, 8)];
    /// const V2_FIELDS: [Field; 2] = [Field::bits("id", 0, 8), Field::bits("flags", 8, 8)];
    /// const V1: Layout<2> = Layout::new(&V1_FIELDS);
    /// const V2: Layout<2> = Layout::new(&V2_FIELDS).with_version(2);
    /// const _: () = assert!(V2.extends(&V1));
    /// ```
    pub const fn extends<const C: usize>(&self, base: &Layout<C>) -> bool {
        if self.version <= base.version {
            return false;
        }
        let mut i = 0;
        while i < base.fields.len() {
            let g = base.fields[i];
            let mut found = false;
            let mut j = 0;
            while j < self.fields.len() {
                let f = self.fields[j];
                if str_eq(f.name, g.name) {
                    found = f.offset == g.offset
                        && f.width == g.width
                        && (f.endian as u8) == (g.endian as u8);
                    break;
                }
                j += 1;
            }
            if !found {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Writes `values`, one per field in declaration order, into `out`.
    ///
    /// Only the first `B` bytes are written. Bits not covered by a field are zero.
    pub fn encode(&self, values: &[u64], out: &mut [u8]) -> Result<usize, CodecError> {
        if values.len() != self.fields.len() {
            return Err(CodecError::WrongValueCount {
                expected: self.fields.len(),
                got: values.len(),
            });
        }
        if out.len() < B {
            return Err(CodecError::BufferTooSmall { needed: B });
        }
        for (i, (f, &v)) in self.fields.iter().zip(values).enumerate() {
            if f.width < 64 && v >> f.width != 0 {
                return Err(CodecError::TooWide {
                    field: i,
                    width: f.width,
                });
            }
        }
        let out = &mut out[..B];
        out.fill(0);
        for (f, &v) in self.fields.iter().zip(values) {
            write_field(out, f, v);
        }
        Ok(B)
    }

    /// Reads one value per field from `buf` into `out`.
    pub fn decode(&self, buf: &[u8], out: &mut [u64]) -> Result<(), CodecError> {
        if out.len() != self.fields.len() {
            return Err(CodecError::WrongValueCount {
                expected: self.fields.len(),
                got: out.len(),
            });
        }
        if buf.len() < B {
            return Err(CodecError::BufferTooSmall { needed: B });
        }
        for (f, slot) in self.fields.iter().zip(out.iter_mut()) {
            *slot = read_field(&buf[..B], f);
        }
        Ok(())
    }
}

fn write_field(buf: &mut [u8], f: &Field, v: u64) {
    let offset = usize::from(f.offset);
    match f.endian {
        Endian::Little => {
            for i in 0..usize::from(f.width) {
                if (v >> i) & 1 == 1 {
                    let pos = offset + i;
                    buf[pos / 8] |= 1 << (pos % 8);
                }
            }
        }
        Endian::Big => {
            // Validated: byte-aligned and a whole number of bytes.
            let n = usize::from(f.width) / 8;
            for k in 0..n {
                let shift = 8 * (n - 1 - k);
                buf[offset / 8 + k] = ((v >> shift) & 0xFF) as u8;
            }
        }
    }
}

fn read_field(buf: &[u8], f: &Field) -> u64 {
    let offset = usize::from(f.offset);
    match f.endian {
        Endian::Little => {
            let mut v = 0u64;
            for i in 0..usize::from(f.width) {
                let pos = offset + i;
                if (buf[pos / 8] >> (pos % 8)) & 1 == 1 {
                    v |= 1 << i;
                }
            }
            v
        }
        Endian::Big => {
            let n = usize::from(f.width) / 8;
            let mut v = 0u64;
            for k in 0..n {
                v = (v << 8) | u64::from(buf[offset / 8 + k]);
            }
            v
        }
    }
}

//! Field descriptions and the const checks that validate them.

/// Byte order of a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endian {
    /// Bits are packed from the least significant bit of byte 0 upward. Any width is allowed.
    Little,
    /// Whole bytes, most significant first. Needs a byte-aligned offset and a width that is a multiple of 8.
    Big,
}

/// One named field: `width` bits starting at bit `offset` of the layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    /// Field name. Names must be unique within a layout.
    pub name: &'static str,
    /// Bit offset from the start of the layout.
    pub offset: u16,
    /// Width in bits, from 1 to 64.
    pub width: u8,
    /// Byte order.
    pub endian: Endian,
}

impl Field {
    /// A little-endian bit field.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_codec::Field;
    ///
    /// let field = Field::bits("flags", 0, 4);
    /// assert_eq!((field.name, field.offset, field.width), ("flags", 0, 4));
    /// ```
    pub const fn bits(name: &'static str, offset: u16, width: u8) -> Self {
        Self {
            name,
            offset,
            width,
            endian: Endian::Little,
        }
    }

    /// A big-endian, byte-aligned field.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_codec::{Endian, Field};
    ///
    /// let field = Field::be("sensor", 16, 16);
    /// assert_eq!(field.endian, Endian::Big);
    /// assert_eq!(field.offset, 16);
    /// ```
    pub const fn be(name: &'static str, offset: u16, width: u8) -> Self {
        Self {
            name,
            offset,
            width,
            endian: Endian::Big,
        }
    }
}

/// Checks a field list for a layout of `bytes` bytes. Panics with a message
/// if the list is invalid. Called in a const context, so the panic is a
/// compile error.
pub(crate) const fn validate(fields: &[Field], bytes: usize) {
    assert!(bytes > 0, "a layout needs at least one byte");
    assert!(fields.len() <= 64, "a layout holds at most 64 fields");
    let total_bits = bytes * 8;
    let mut i = 0;
    while i < fields.len() {
        let f = fields[i];
        assert!(
            f.width >= 1 && f.width <= 64,
            "field width must be 1 to 64 bits"
        );
        assert!(
            (f.offset as usize) + (f.width as usize) <= total_bits,
            "field extends past the end of the layout"
        );
        if let Endian::Big = f.endian {
            assert!(
                f.offset.is_multiple_of(8) && f.width.is_multiple_of(8),
                "a big-endian field must be byte-aligned and a whole number of bytes"
            );
        }
        assert!(!str_is_empty(f.name), "field names must not be empty");
        // Overlap and name checks against every later field.
        let mut j = i + 1;
        while j < fields.len() {
            let g = fields[j];
            let disjoint = (f.offset as usize) + (f.width as usize) <= g.offset as usize
                || (g.offset as usize) + (g.width as usize) <= f.offset as usize;
            assert!(disjoint, "two fields overlap");
            assert!(!str_eq(f.name, g.name), "two fields share a name");
            j += 1;
        }
        i += 1;
    }
}

pub(crate) const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn str_is_empty(s: &str) -> bool {
    s.is_empty()
}

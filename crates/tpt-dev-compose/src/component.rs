//! A parser for the binary format of WebAssembly components.
//!
//! This reads the component's sections: their ids, offsets and sizes, the item
//! count of vector sections, custom section names, embedded core modules, and
//! nested components. It parses embedded core modules with [`crate::module`].
//!
//! It does not decode the contents of the vector sections. Their items (types,
//! imports, exports, aliases, canonical functions and instances) are not read,
//! so their names and signatures are not available here. Section order is not
//! enforced, since components may place sections in any order.

use alloc::string::String;
use alloc::vec::Vec;

use crate::module::{self, ErrorKind, Module, ParseError};
use crate::types::{self, TypeDef};

/// The deepest nesting of components the parser accepts.
pub const MAX_DEPTH: usize = 16;

/// A parsed component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    /// The sections, in the order they appear.
    pub sections: Vec<Section>,
}

/// One section of a component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// The section id, from 0 to 12.
    pub id: u8,
    /// Byte offset of the section id within the component.
    pub offset: usize,
    /// Size of the section contents, in bytes.
    pub size: usize,
    /// For vector sections, the number of items. `None` for the others.
    pub count: Option<u32>,
    /// For custom sections (id 0), the section name.
    pub custom_name: Option<String>,
    /// For core module sections (id 1), the parsed module.
    pub core_module: Option<Module>,
    /// For nested component sections (id 4), the parsed component.
    pub nested: Option<Component>,
    /// For type sections (id 7), the decoded types, in order. `None` if the
    /// section holds a type kind this parser does not decode (instance,
    /// component or resource types, or a module type).
    pub types: Option<Vec<TypeDef>>,
}

impl Section {
    /// The name the specification gives to a section id.
    pub fn kind_name(id: u8) -> &'static str {
        match id {
            0 => "custom",
            1 => "core module",
            2 => "core instance",
            3 => "core type",
            4 => "component",
            5 => "instance",
            6 => "alias",
            7 => "type",
            8 => "canonical function",
            9 => "start",
            10 => "import",
            11 => "export",
            12 => "value",
            _ => "unknown",
        }
    }
}

/// Reads a component from its binary form.
pub fn parse(bytes: &[u8]) -> Result<Component, ParseError> {
    parse_at(bytes, 0, 0)
}

fn err(offset: usize, kind: ErrorKind) -> ParseError {
    ParseError { offset, kind }
}

fn parse_at(bytes: &[u8], base: usize, depth: usize) -> Result<Component, ParseError> {
    if depth > MAX_DEPTH {
        return Err(err(base, ErrorKind::NestingTooDeep));
    }
    match bytes.get(0..4) {
        Some(magic) if magic == b"\0asm" => {}
        _ => return Err(err(base, ErrorKind::BadMagic)),
    }
    match bytes.get(4..6) {
        Some([0x0d, 0x00]) => {}
        Some(_) => return Err(err(base + 4, ErrorKind::BadVersion)),
        None => return Err(err(base + 4, ErrorKind::Truncated)),
    }
    match bytes.get(6..8) {
        Some([0x01, 0x00]) => {}
        Some(_) => return Err(err(base + 6, ErrorKind::NotAComponent)),
        None => return Err(err(base + 6, ErrorKind::Truncated)),
    }

    let mut r = Reader {
        bytes,
        pos: 8,
        base,
    };
    let mut sections = Vec::new();
    while !r.done() {
        let offset = r.base + r.pos;
        let id = r.byte()?;
        let size = r.u32()? as usize;
        let body_start = r.pos;
        let body = r.take(size)?;
        let body_base = r.base + body_start;

        let mut section = Section {
            id,
            offset,
            size,
            count: None,
            custom_name: None,
            core_module: None,
            nested: None,
            types: None,
        };
        match id {
            0 => {
                let mut s = Reader {
                    bytes: body,
                    pos: 0,
                    base: body_base,
                };
                section.custom_name = Some(s.name()?);
            }
            1 => {
                let parsed = module::parse(body).map_err(|e| ParseError {
                    offset: e.offset + body_base,
                    kind: e.kind,
                })?;
                section.core_module = Some(parsed);
            }
            4 => {
                section.nested = Some(parse_at(body, body_base, depth + 1)?);
            }
            7 => {
                let mut s = Reader {
                    bytes: body,
                    pos: 0,
                    base: body_base,
                };
                let n = s.count()?;
                section.count = Some(n);
                section.types = types::decode_items(&mut s, n)?;
                if section.types.is_some() && !s.done() {
                    return Err(s.err(ErrorKind::TrailingBytes));
                }
            }
            2 | 3 | 5 | 6 | 8 | 10 | 11 | 12 => {
                let mut s = Reader {
                    bytes: body,
                    pos: 0,
                    base: body_base,
                };
                section.count = Some(s.count()?);
            }
            9 => {}
            other => return Err(err(offset, ErrorKind::BadSectionId(other))),
        }
        sections.push(section);
    }
    Ok(Component { sections })
}

pub(crate) struct Reader<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) pos: usize,
    pub(crate) base: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn err(&self, kind: ErrorKind) -> ParseError {
        err(self.base + self.pos, kind)
    }

    pub(crate) fn done(&self) -> bool {
        self.pos == self.bytes.len()
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    pub(crate) fn peek(&self) -> Result<u8, ParseError> {
        self.bytes
            .get(self.pos)
            .copied()
            .ok_or(self.err(ErrorKind::Truncated))
    }

    pub(crate) fn byte(&mut self) -> Result<u8, ParseError> {
        let b = self.peek()?;
        self.pos += 1;
        Ok(b)
    }

    /// A signed LEB128 integer of at most 33 bits, as used for value type indices.
    pub(crate) fn s33(&mut self) -> Result<i64, ParseError> {
        let mut value = 0i64;
        let mut shift = 0u32;
        loop {
            let b = self.byte()?;
            value |= i64::from(b & 0x7F) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                if (b & 0x40) != 0 {
                    value |= -1i64 << shift;
                }
                return Ok(value);
            }
            // Five bytes cover 35 bits, which is more than 33, so a sixth is too long.
            if shift >= 35 {
                return Err(self.err(ErrorKind::BadLeb128));
            }
        }
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        if n > self.remaining() {
            return Err(self.err(ErrorKind::Truncated));
        }
        let out = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, ParseError> {
        let mut value = 0u32;
        let mut shift = 0u32;
        loop {
            let b = self.byte()?;
            if shift == 28 && (b & 0xF0) != 0 {
                return Err(self.err(ErrorKind::BadLeb128));
            }
            value |= u32::from(b & 0x7F) << shift;
            if b & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
    }

    /// An item count. Each item takes at least one byte, so the count cannot exceed the bytes left.
    pub(crate) fn count(&mut self) -> Result<u32, ParseError> {
        let n = self.u32()?;
        if n as usize > self.remaining() {
            return Err(self.err(ErrorKind::CountTooLarge));
        }
        Ok(n)
    }

    pub(crate) fn name(&mut self) -> Result<String, ParseError> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        match core::str::from_utf8(bytes) {
            Ok(s) => Ok(String::from(s)),
            Err(_) => Err(self.err(ErrorKind::BadName)),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const PLAIN: [u8; 8] = [0, b'a', b's', b'm', 0x0d, 0, 0x01, 0];

    /// Encodes `n` as an unsigned LEB128 integer.
    fn leb(mut n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let low = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(low);
                return out;
            }
            out.push(low | 0x80);
        }
    }

    /// Wraps `inner` as a nested-component section (id 4) of an empty component.
    fn wrap(inner: &[u8]) -> Vec<u8> {
        let mut out = PLAIN.to_vec();
        out.push(4);
        out.extend(leb(inner.len()));
        out.extend_from_slice(inner);
        out
    }

    #[test]
    fn an_empty_component_has_no_sections() {
        assert_eq!(parse(&PLAIN).unwrap().sections, Vec::new());
    }

    #[test]
    fn a_core_module_layer_is_not_a_component() {
        let mut bytes = PLAIN;
        bytes[6] = 0;
        assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::NotAComponent);
    }

    #[test]
    fn a_bad_version_is_reported() {
        let mut bytes = PLAIN;
        bytes[4] = 0x0c;
        assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::BadVersion);
    }

    #[test]
    fn an_unknown_section_id_is_reported_at_its_offset() {
        let mut bytes = PLAIN.to_vec();
        bytes.extend_from_slice(&[13, 0]);
        let e = parse(&bytes).unwrap_err();
        assert_eq!(e.kind, ErrorKind::BadSectionId(13));
        assert_eq!(e.offset, 8);
    }

    #[test]
    fn a_section_longer_than_the_input_is_truncated() {
        let mut bytes = PLAIN.to_vec();
        bytes.extend_from_slice(&[7, 0x10]);
        assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::Truncated);
    }

    #[test]
    fn a_count_larger_than_the_section_is_refused() {
        let mut bytes = PLAIN.to_vec();
        // A type section (7) of size 1 whose count is 5.
        bytes.extend_from_slice(&[7, 1, 5]);
        assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::CountTooLarge);
    }

    #[test]
    fn nesting_beyond_the_limit_is_refused() {
        let mut bytes = PLAIN.to_vec();
        for _ in 0..=MAX_DEPTH + 1 {
            bytes = wrap(&bytes);
        }
        assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::NestingTooDeep);
    }

    #[test]
    fn nesting_within_the_limit_is_accepted() {
        let mut bytes = PLAIN.to_vec();
        for _ in 0..3 {
            bytes = wrap(&bytes);
        }
        let outer = parse(&bytes).unwrap();
        assert_eq!(outer.sections.len(), 1);
        assert_eq!(outer.sections[0].id, 4);
        assert!(outer.sections[0].nested.is_some());
    }
}

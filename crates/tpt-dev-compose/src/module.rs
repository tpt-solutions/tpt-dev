//! A parser for the binary format of core WebAssembly modules.
//!
//! It reads the sections that linking needs: types, imports, function
//! signatures, exports and the start function. It also checks the code section
//! and the section order. Other sections are checked for length, then skipped.
//! Instruction bodies are never decoded, so this parser does not validate code.
//! The indices of tables, memories, globals and tags are not bounds-checked.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// A value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValType {
    /// 32-bit integer.
    I32,
    /// 64-bit integer.
    I64,
    /// 32-bit float.
    F32,
    /// 64-bit float.
    F64,
    /// 128-bit vector.
    V128,
    /// Function reference.
    FuncRef,
    /// External reference.
    ExternRef,
}

/// The signature of a function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncType {
    /// Parameter types, in order.
    pub params: Vec<ValType>,
    /// Result types, in order.
    pub results: Vec<ValType>,
}

/// What an import provides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportDesc {
    /// A function, with its index into the module's types.
    Func(u32),
    /// A table.
    Table,
    /// A memory.
    Memory,
    /// A global.
    Global,
    /// An exception tag.
    Tag,
}

/// One import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    /// The module the import comes from.
    pub module: String,
    /// The name within that module.
    pub name: String,
    /// What is imported.
    pub desc: ImportDesc,
}

/// The kind of thing an export names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    /// A function.
    Func,
    /// A table.
    Table,
    /// A memory.
    Memory,
    /// A global.
    Global,
    /// An exception tag.
    Tag,
}

/// One export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    /// The exported name.
    pub name: String,
    /// What kind of thing is exported.
    pub kind: ExportKind,
    /// Index into that kind's index space.
    pub index: u32,
}

/// A parsed core module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    /// Function types.
    pub types: Vec<FuncType>,
    /// Imports, in order.
    pub imports: Vec<Import>,
    /// Type index of each function the module defines, in order.
    pub functions: Vec<u32>,
    /// Exports, in order.
    pub exports: Vec<Export>,
    /// The start function, if any.
    pub start: Option<u32>,
    /// Names of the custom sections, in order.
    pub custom_sections: Vec<String>,
}

impl Module {
    /// The number of functions: imported ones, then defined ones.
    pub fn func_count(&self) -> usize {
        self.func_imports().count() + self.functions.len()
    }

    fn func_imports(&self) -> impl Iterator<Item = u32> + '_ {
        self.imports.iter().filter_map(|i| match i.desc {
            ImportDesc::Func(t) => Some(t),
            _ => None,
        })
    }

    /// The signature of function `index`, in the function index space.
    pub fn func_type(&self, index: u32) -> Option<&FuncType> {
        let index = index as usize;
        let imported: Vec<u32> = self.func_imports().collect();
        let type_index = if index < imported.len() {
            imported[index]
        } else {
            *self.functions.get(index - imported.len())?
        };
        self.types.get(type_index as usize)
    }
}

/// Why a module could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// The file does not start with the `\0asm` magic.
    BadMagic,
    /// The version is not 1.
    BadVersion,
    /// The input ends before a field is complete.
    Truncated,
    /// A LEB128 integer is too long or has bits beyond 32.
    BadLeb128,
    /// A section id is unknown.
    BadSectionId(u8),
    /// A section appears out of order, or twice.
    BadSectionOrder,
    /// A section has bytes left over after its contents.
    TrailingBytes,
    /// A value type byte is not known.
    BadValType(u8),
    /// A function type does not start with 0x60.
    BadFuncType(u8),
    /// An export kind byte is not known.
    BadExportKind(u8),
    /// An import kind byte is not known.
    BadImportKind(u8),
    /// A name is not valid UTF-8.
    BadName,
    /// A limits flag is not one this parser supports.
    BadLimits(u8),
    /// A type index is out of range.
    BadTypeIndex(u32),
    /// A function index is out of range.
    BadFuncIndex(u32),
    /// The function and code sections disagree about how many functions there are.
    CodeCountMismatch,
    /// The element count is larger than the bytes left in the section.
    CountTooLarge,
}

/// A parse error, with the byte offset where it was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Byte offset into the module.
    pub offset: usize,
    /// What went wrong.
    pub kind: ErrorKind,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} at byte {}", self.kind, self.offset)
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    base: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], base: usize) -> Self {
        Self {
            bytes,
            pos: 0,
            base,
        }
    }

    fn err(&self, kind: ErrorKind) -> ParseError {
        ParseError {
            offset: self.base + self.pos,
            kind,
        }
    }

    fn done(&self) -> bool {
        self.pos == self.bytes.len()
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    fn byte(&mut self) -> Result<u8, ParseError> {
        let b = *self
            .bytes
            .get(self.pos)
            .ok_or(self.err(ErrorKind::Truncated))?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        if n > self.remaining() {
            return Err(self.err(ErrorKind::Truncated));
        }
        let out = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, ParseError> {
        let mut value = 0u32;
        let mut shift = 0u32;
        loop {
            let b = self.byte()?;
            if shift == 28 && (b & 0xF0) != 0 {
                // The fifth byte may carry only the top four bits, and no continuation.
                return Err(self.err(ErrorKind::BadLeb128));
            }
            value |= u32::from(b & 0x7F) << shift;
            if b & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
    }

    /// A count of elements. Each element takes at least one byte, so the count
    /// cannot exceed the bytes left.
    fn count(&mut self) -> Result<usize, ParseError> {
        let n = self.u32()? as usize;
        if n > self.remaining() {
            return Err(self.err(ErrorKind::CountTooLarge));
        }
        Ok(n)
    }

    fn name(&mut self) -> Result<String, ParseError> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        match core::str::from_utf8(bytes) {
            Ok(s) => Ok(String::from(s)),
            Err(_) => Err(self.err(ErrorKind::BadName)),
        }
    }

    fn valtype(&mut self) -> Result<ValType, ParseError> {
        let b = self.byte()?;
        Ok(match b {
            0x7F => ValType::I32,
            0x7E => ValType::I64,
            0x7D => ValType::F32,
            0x7C => ValType::F64,
            0x7B => ValType::V128,
            0x70 => ValType::FuncRef,
            0x6F => ValType::ExternRef,
            other => return Err(self.err(ErrorKind::BadValType(other))),
        })
    }

    fn reftype(&mut self) -> Result<(), ParseError> {
        match self.byte()? {
            0x70 | 0x6F => Ok(()),
            other => Err(self.err(ErrorKind::BadValType(other))),
        }
    }

    /// Skips a limits field. Only the 32-bit forms are accepted.
    fn limits(&mut self) -> Result<(), ParseError> {
        match self.byte()? {
            0x00 => {
                self.u32()?;
            }
            0x01 => {
                self.u32()?;
                self.u32()?;
            }
            other => return Err(self.err(ErrorKind::BadLimits(other))),
        }
        Ok(())
    }

    fn func_type(&mut self) -> Result<FuncType, ParseError> {
        match self.byte()? {
            0x60 => {}
            other => return Err(self.err(ErrorKind::BadFuncType(other))),
        }
        let np = self.count()?;
        let mut params = Vec::with_capacity(np);
        for _ in 0..np {
            params.push(self.valtype()?);
        }
        let nr = self.count()?;
        let mut results = Vec::with_capacity(nr);
        for _ in 0..nr {
            results.push(self.valtype()?);
        }
        Ok(FuncType { params, results })
    }

    fn type_index(&mut self, types: usize) -> Result<u32, ParseError> {
        let t = self.u32()?;
        if t as usize >= types {
            return Err(self.err(ErrorKind::BadTypeIndex(t)));
        }
        Ok(t)
    }
}

/// The order sections must appear in. Data count comes after element and
/// before code, as the specification requires.
fn rank(id: u8) -> Option<u8> {
    Some(match id {
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 5,
        6 => 6,
        7 => 7,
        8 => 8,
        9 => 9,
        12 => 10,
        10 => 11,
        11 => 12,
        _ => return None,
    })
}

/// Parses a core module from its binary form.
pub fn parse(bytes: &[u8]) -> Result<Module, ParseError> {
    let mut r = Reader::new(bytes, 0);
    let magic = bytes.get(0..4).ok_or(ParseError {
        offset: 0,
        kind: ErrorKind::BadMagic,
    })?;
    if magic != b"\0asm" {
        return Err(ParseError {
            offset: 0,
            kind: ErrorKind::BadMagic,
        });
    }
    let version = bytes.get(4..8).ok_or(ParseError {
        offset: 4,
        kind: ErrorKind::Truncated,
    })?;
    if version != [1, 0, 0, 0] {
        return Err(ParseError {
            offset: 4,
            kind: ErrorKind::BadVersion,
        });
    }
    r.pos = 8;

    let mut module = Module::default();
    let mut last_rank = 0u8;
    let mut code_count: Option<usize> = None;
    let mut function_count: Option<usize> = None;

    while !r.done() {
        let id = r.byte()?;
        let size = r.u32()? as usize;
        let body_base = r.base + r.pos;
        let body = r.take(size)?;
        let mut s = Reader::new(body, body_base);

        if id == 0 {
            let name = s.name()?;
            module.custom_sections.push(name);
            continue;
        }
        let current = rank(id).ok_or(r.err(ErrorKind::BadSectionId(id)))?;
        if current <= last_rank {
            return Err(ParseError {
                offset: body_base - 1,
                kind: ErrorKind::BadSectionOrder,
            });
        }
        last_rank = current;

        match id {
            1 => {
                let n = s.count()?;
                for _ in 0..n {
                    module.types.push(s.func_type()?);
                }
            }
            2 => {
                let n = s.count()?;
                for _ in 0..n {
                    let module_name = s.name()?;
                    let name = s.name()?;
                    let desc = match s.byte()? {
                        0x00 => ImportDesc::Func(s.type_index(module.types.len())?),
                        0x01 => {
                            s.reftype()?;
                            s.limits()?;
                            ImportDesc::Table
                        }
                        0x02 => {
                            s.limits()?;
                            ImportDesc::Memory
                        }
                        0x03 => {
                            s.valtype()?;
                            s.byte()?;
                            ImportDesc::Global
                        }
                        0x04 => {
                            s.byte()?;
                            s.u32()?;
                            ImportDesc::Tag
                        }
                        other => return Err(s.err(ErrorKind::BadImportKind(other))),
                    };
                    module.imports.push(Import {
                        module: module_name,
                        name,
                        desc,
                    });
                }
            }
            3 => {
                let n = s.count()?;
                function_count = Some(n);
                for _ in 0..n {
                    let t = s.type_index(module.types.len())?;
                    module.functions.push(t);
                }
            }
            7 => {
                let n = s.count()?;
                for _ in 0..n {
                    let name = s.name()?;
                    let kind = match s.byte()? {
                        0x00 => ExportKind::Func,
                        0x01 => ExportKind::Table,
                        0x02 => ExportKind::Memory,
                        0x03 => ExportKind::Global,
                        0x04 => ExportKind::Tag,
                        other => return Err(s.err(ErrorKind::BadExportKind(other))),
                    };
                    let index = s.u32()?;
                    module.exports.push(Export { name, kind, index });
                }
            }
            8 => {
                let f = s.u32()?;
                module.start = Some(f);
            }
            10 => {
                let n = s.count()?;
                code_count = Some(n);
                for _ in 0..n {
                    let len = s.u32()? as usize;
                    s.take(len)?;
                }
            }
            // Table, memory, global, element, data and data count sections are
            // length-checked and skipped.
            _ => {
                s.pos = s.bytes.len();
            }
        }
        if !s.done() {
            return Err(s.err(ErrorKind::TrailingBytes));
        }
    }

    let defined = function_count.unwrap_or(0);
    if defined != code_count.unwrap_or(0) {
        return Err(ParseError {
            offset: bytes.len(),
            kind: ErrorKind::CodeCountMismatch,
        });
    }
    if let Some(start) = module.start
        && start as usize >= module.func_count()
    {
        return Err(ParseError {
            offset: bytes.len(),
            kind: ErrorKind::BadFuncIndex(start),
        });
    }
    for export in &module.exports {
        if export.kind == ExportKind::Func && export.index as usize >= module.func_count() {
            return Err(ParseError {
                offset: bytes.len(),
                kind: ErrorKind::BadFuncIndex(export.index),
            });
        }
    }
    Ok(module)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const EMPTY: [u8; 8] = [0, b'a', b's', b'm', 1, 0, 0, 0];

    #[test]
    fn the_empty_module_parses() {
        assert_eq!(parse(&EMPTY).unwrap(), Module::default());
    }

    #[test]
    fn bad_magic_and_version_are_reported() {
        let mut bad = EMPTY;
        bad[0] = 1;
        assert_eq!(parse(&bad).unwrap_err().kind, ErrorKind::BadMagic);
        let mut version = EMPTY;
        version[4] = 2;
        assert_eq!(parse(&version).unwrap_err().kind, ErrorKind::BadVersion);
    }

    #[test]
    fn a_section_longer_than_the_input_is_truncated() {
        let mut bytes = EMPTY.to_vec();
        bytes.extend_from_slice(&[1, 0x10]);
        assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::Truncated);
    }

    #[test]
    fn sections_out_of_order_are_rejected() {
        // A function section (id 3, empty) followed by a type section (id 1, empty).
        let mut bytes = EMPTY.to_vec();
        bytes.extend_from_slice(&[3, 1, 0, 1, 1, 0]);
        assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::BadSectionOrder);
    }

    #[test]
    fn a_long_leb128_is_rejected() {
        // A section size encoded in six bytes.
        let mut bytes = EMPTY.to_vec();
        bytes.extend_from_slice(&[1, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00]);
        assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::BadLeb128);
    }

    #[test]
    fn functions_without_code_are_rejected() {
        // A type section with one "() -> ()" type, and a function section naming it.
        let mut bytes = EMPTY.to_vec();
        bytes.extend_from_slice(&[1, 4, 1, 0x60, 0, 0]);
        bytes.extend_from_slice(&[3, 2, 1, 0]);
        assert_eq!(
            parse(&bytes).unwrap_err().kind,
            ErrorKind::CodeCountMismatch
        );
    }

    #[test]
    fn a_bad_type_index_is_rejected() {
        let mut bytes = EMPTY.to_vec();
        bytes.extend_from_slice(&[3, 2, 1, 5]);
        bytes.extend_from_slice(&[10, 1, 0]);
        assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::BadTypeIndex(5));
    }

    #[test]
    fn a_bad_value_type_is_rejected() {
        let mut bytes = EMPTY.to_vec();
        bytes.extend_from_slice(&[1, 5, 1, 0x60, 1, 0x00, 0]);
        assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::BadValType(0x00));
    }
}

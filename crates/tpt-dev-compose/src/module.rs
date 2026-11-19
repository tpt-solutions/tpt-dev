//! A parser for the binary format of core WebAssembly modules.
//!
//! It reads the sections that linking needs: types, imports, function
//! signatures, tables, memories, globals, exports and the start function. It
//! also checks the code section and the section order. Element and data
//! sections are checked for length, then skipped. Instruction bodies are never
//! decoded, so this parser does not validate code. Initialiser expressions are
//! skipped after checking their opcodes.
//!
//! Every index in the module is bounds-checked against its index space: types,
//! functions, tables, memories, globals and tags, including those an import
//! adds. Export names must be unique.

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

/// The size limits of a table or memory. Units are elements or pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The smallest size.
    pub min: u32,
    /// The largest size, if bounded.
    pub max: Option<u32>,
}

/// The type of a table: its element type and size limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableType {
    /// The element type, which is `FuncRef` or `ExternRef`.
    pub elem: ValType,
    /// The size limits.
    pub limits: Limits,
}

/// The type of a global: its value type and whether it is mutable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalType {
    /// The value type.
    pub ty: ValType,
    /// Whether the global can be written.
    pub mutable: bool,
}

/// What an import provides, with the type it must have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportDesc {
    /// A function, with its index into the module's types.
    Func(u32),
    /// A table.
    Table(TableType),
    /// A memory, in pages.
    Memory(Limits),
    /// A global.
    Global(GlobalType),
    /// An exception tag, with the index of its function type.
    Tag(u32),
}

impl ImportDesc {
    /// The kind of export this import must be matched with.
    pub fn kind(&self) -> ExportKind {
        match self {
            ImportDesc::Func(_) => ExportKind::Func,
            ImportDesc::Table(_) => ExportKind::Table,
            ImportDesc::Memory(_) => ExportKind::Memory,
            ImportDesc::Global(_) => ExportKind::Global,
            ImportDesc::Tag(_) => ExportKind::Tag,
        }
    }
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
///
/// The `tables`, `memories` and `globals` fields hold only the entries the
/// module defines. Imported entries are in `imports`. The `*_type` methods
/// give the type of any entry in each index space, imported ones first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    /// Function types.
    pub types: Vec<FuncType>,
    /// Imports, in order.
    pub imports: Vec<Import>,
    /// Type index of each function the module defines, in order.
    pub functions: Vec<u32>,
    /// Each table the module defines, in order.
    pub tables: Vec<TableType>,
    /// Each memory the module defines, in order.
    pub memories: Vec<Limits>,
    /// Each global the module defines, in order.
    pub globals: Vec<GlobalType>,
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

    /// The number of entries in the table index space.
    pub fn table_count(&self) -> usize {
        self.import_descs(ExportKind::Table).count() + self.tables.len()
    }

    /// The number of entries in the memory index space.
    pub fn memory_count(&self) -> usize {
        self.import_descs(ExportKind::Memory).count() + self.memories.len()
    }

    /// The number of entries in the global index space.
    pub fn global_count(&self) -> usize {
        self.import_descs(ExportKind::Global).count() + self.globals.len()
    }

    /// The number of entries in the tag index space.
    pub fn tag_count(&self) -> usize {
        self.import_descs(ExportKind::Tag).count()
    }

    fn import_descs(&self, kind: ExportKind) -> impl Iterator<Item = ImportDesc> + '_ {
        self.imports
            .iter()
            .map(|i| i.desc)
            .filter(move |d| d.kind() == kind)
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

    /// The type of table `index`, in the table index space.
    pub fn table_type(&self, index: u32) -> Option<TableType> {
        let index = index as usize;
        let imported = self.import_descs(ExportKind::Table).count();
        if index < imported {
            return self
                .import_descs(ExportKind::Table)
                .nth(index)
                .and_then(|d| match d {
                    ImportDesc::Table(t) => Some(t),
                    _ => None,
                });
        }
        self.tables.get(index - imported).copied()
    }

    /// The limits of memory `index`, in the memory index space.
    pub fn memory_type(&self, index: u32) -> Option<Limits> {
        let index = index as usize;
        let imported = self.import_descs(ExportKind::Memory).count();
        if index < imported {
            return self
                .import_descs(ExportKind::Memory)
                .nth(index)
                .and_then(|d| match d {
                    ImportDesc::Memory(l) => Some(l),
                    _ => None,
                });
        }
        self.memories.get(index - imported).copied()
    }

    /// The type of global `index`, in the global index space.
    pub fn global_type(&self, index: u32) -> Option<GlobalType> {
        let index = index as usize;
        let imported = self.import_descs(ExportKind::Global).count();
        if index < imported {
            return self
                .import_descs(ExportKind::Global)
                .nth(index)
                .and_then(|d| match d {
                    ImportDesc::Global(g) => Some(g),
                    _ => None,
                });
        }
        self.globals.get(index - imported).copied()
    }

    /// The function type index of tag `index`, in the tag index space.
    pub fn tag_type(&self, index: u32) -> Option<u32> {
        self.import_descs(ExportKind::Tag)
            .nth(index as usize)
            .and_then(|d| match d {
                ImportDesc::Tag(t) => Some(t),
                _ => None,
            })
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
    /// A component value type is neither a primitive nor a valid type index.
    BadValueType(u8),
    /// A component type has a flag or tag byte outside the forms it allows.
    BadTypeEncoding(u8),
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
    /// A limits field has a minimum above its maximum.
    LimitsOrder,
    /// A memory has more pages than the 32-bit address space allows.
    MemoryTooLarge(u32),
    /// A global's mutability byte is neither 0 nor 1.
    BadMutability(u8),
    /// A constant expression uses an instruction this parser does not allow.
    BadConstOp(u8),
    /// A table's reference type byte is not a reference type.
    BadRefType(u8),
    /// An exception tag's attribute byte is not 0.
    BadTagAttribute,
    /// A type index is out of range.
    BadTypeIndex(u32),
    /// A function index is out of range.
    BadFuncIndex(u32),
    /// A table index is out of range.
    BadTableIndex(u32),
    /// A memory index is out of range.
    BadMemoryIndex(u32),
    /// A global index is out of range.
    BadGlobalIndex(u32),
    /// A tag index is out of range.
    BadTagIndex(u32),
    /// Two exports have the same name.
    DuplicateExport,
    /// The function and code sections disagree about how many functions there are.
    CodeCountMismatch,
    /// The element count is larger than the bytes left in the section.
    CountTooLarge,
    /// The input is a core module, not a component: its layer is not 1.
    NotAComponent,
    /// Components are nested deeper than the parser allows.
    NestingTooDeep,
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

    fn reftype(&mut self) -> Result<ValType, ParseError> {
        match self.byte()? {
            0x70 => Ok(ValType::FuncRef),
            0x6F => Ok(ValType::ExternRef),
            other => Err(self.err(ErrorKind::BadRefType(other))),
        }
    }

    /// Reads a limits field. Only the 32-bit forms are accepted.
    fn limits(&mut self) -> Result<Limits, ParseError> {
        let flag = self.byte()?;
        let min = self.u32()?;
        let max = match flag {
            0x00 => None,
            0x01 => Some(self.u32()?),
            other => return Err(self.err(ErrorKind::BadLimits(other))),
        };
        if let Some(m) = max
            && m < min
        {
            return Err(self.err(ErrorKind::LimitsOrder));
        }
        Ok(Limits { min, max })
    }

    /// Reads a memory's limits, which must fit in the 32-bit address space.
    fn memory_limits(&mut self) -> Result<Limits, ParseError> {
        let l = self.limits()?;
        let too_big = |n: u32| n > 65_536;
        if too_big(l.min) {
            return Err(self.err(ErrorKind::MemoryTooLarge(l.min)));
        }
        if let Some(m) = l.max
            && too_big(m)
        {
            return Err(self.err(ErrorKind::MemoryTooLarge(m)));
        }
        Ok(l)
    }

    fn global_type(&mut self) -> Result<GlobalType, ParseError> {
        let ty = self.valtype()?;
        let mutable = match self.byte()? {
            0 => false,
            1 => true,
            other => return Err(self.err(ErrorKind::BadMutability(other))),
        };
        Ok(GlobalType { ty, mutable })
    }

    /// Skips a signed LEB128 integer, which the constant expressions use.
    fn skip_sleb(&mut self) -> Result<(), ParseError> {
        for _ in 0..10 {
            if self.byte()? & 0x80 == 0 {
                return Ok(());
            }
        }
        Err(self.err(ErrorKind::BadLeb128))
    }

    /// Skips a constant expression, checking that it only uses the instructions
    /// the specification allows in one. Its value is not computed.
    fn skip_const_expr(&mut self) -> Result<(), ParseError> {
        loop {
            match self.byte()? {
                0x0B => return Ok(()),
                0x41 | 0x42 => self.skip_sleb()?,
                0x43 => {
                    self.take(4)?;
                }
                0x44 => {
                    self.take(8)?;
                }
                0x23 | 0xD2 => {
                    self.u32()?;
                }
                // ref.null takes a heap type, which is a signed LEB128 integer.
                0xD0 => self.skip_sleb()?,
                0x6A..=0x6C | 0x7C..=0x7E => {}
                other => return Err(self.err(ErrorKind::BadConstOp(other))),
            }
        }
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
                            let elem = s.reftype()?;
                            let limits = s.limits()?;
                            ImportDesc::Table(TableType { elem, limits })
                        }
                        0x02 => ImportDesc::Memory(s.memory_limits()?),
                        0x03 => ImportDesc::Global(s.global_type()?),
                        0x04 => {
                            // The attribute byte is 0 for an exception tag.
                            if s.byte()? != 0 {
                                return Err(s.err(ErrorKind::BadTagAttribute));
                            }
                            ImportDesc::Tag(s.type_index(module.types.len())?)
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
            4 => {
                let n = s.count()?;
                for _ in 0..n {
                    // A table may start with 0x40 0x00 and an initial value expression.
                    let with_init = s.bytes.get(s.pos) == Some(&0x40);
                    if with_init {
                        s.take(2)?;
                    }
                    let elem = s.reftype()?;
                    let limits = s.limits()?;
                    if with_init {
                        s.skip_const_expr()?;
                    }
                    module.tables.push(TableType { elem, limits });
                }
            }
            5 => {
                let n = s.count()?;
                for _ in 0..n {
                    module.memories.push(s.memory_limits()?);
                }
            }
            6 => {
                let n = s.count()?;
                for _ in 0..n {
                    let ty = s.global_type()?;
                    s.skip_const_expr()?;
                    module.globals.push(ty);
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
        let (limit, kind) = match export.kind {
            ExportKind::Func => (module.func_count(), ErrorKind::BadFuncIndex(export.index)),
            ExportKind::Table => (module.table_count(), ErrorKind::BadTableIndex(export.index)),
            ExportKind::Memory => (
                module.memory_count(),
                ErrorKind::BadMemoryIndex(export.index),
            ),
            ExportKind::Global => (
                module.global_count(),
                ErrorKind::BadGlobalIndex(export.index),
            ),
            ExportKind::Tag => (module.tag_count(), ErrorKind::BadTagIndex(export.index)),
        };
        if export.index as usize >= limit {
            return Err(ParseError {
                offset: bytes.len(),
                kind,
            });
        }
    }
    let mut names: Vec<&str> = module.exports.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    if names.windows(2).any(|w| w[0] == w[1]) {
        return Err(ParseError {
            offset: bytes.len(),
            kind: ErrorKind::DuplicateExport,
        });
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

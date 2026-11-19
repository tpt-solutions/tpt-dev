//! A parser for the binary format of WebAssembly components.
//!
//! This reads the component's sections: their ids, offsets and sizes, the item
//! count of vector sections, custom section names, embedded core modules, and
//! nested components. It parses embedded core modules with [`crate::module`].
//!
//! Imports (section 10) and exports (section 11) are decoded: their names, the
//! kind of each, and the index or type descriptor each one carries. Only the
//! kinds core module, func, value, type, instance and component are decoded.
//! Core exports other than modules are refused with [`ErrorKind::UnsupportedSort`].
//!
//! The other vector sections (aliases, canonical functions and instances) are
//! not decoded, so their items are not available here. Section order is not
//! enforced, since components may place sections in any order.
//!
//! Index bounds are checked against an upper bound on each index space. The
//! sections that define entries in a space are counted, along with aliases, which
//! may define entries in any space. An index at or beyond that bound is refused.
//! An index inside the bound is not checked further, because the aliases are not
//! decoded.

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

use crate::module::{self, ErrorKind, Module, ParseError};
use crate::types::{self, TypeDef, ValueType};

/// The deepest nesting of components the parser accepts.
pub const MAX_DEPTH: usize = 16;

/// A parsed component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    /// The sections, in the order they appear.
    pub sections: Vec<Section>,
}

impl Component {
    /// Every import of the component, in the order the sections give them.
    pub fn imports(&self) -> impl Iterator<Item = &ComponentImport> {
        self.sections
            .iter()
            .filter_map(|s| s.imports.as_deref())
            .flatten()
    }

    /// Every export of the component, in the order the sections give them.
    pub fn exports(&self) -> impl Iterator<Item = &ComponentExport> {
        self.sections
            .iter()
            .filter_map(|s| s.exports.as_deref())
            .flatten()
    }

    /// The type at `index` in the component's type index space, if it is a
    /// type this parser decodes.
    ///
    /// Returns `None` when the index names a type that is not decoded, when it
    /// is an import of a type, or when an alias section comes before it, since
    /// an alias may define types and is not decoded.
    pub fn type_def(&self, index: u32) -> Option<&TypeDef> {
        let want = u64::from(index);
        let mut pos: u64 = 0;
        for s in &self.sections {
            match s.id {
                6 => return None,
                7 => {
                    let n = u64::from(s.count.unwrap_or(0));
                    if want < pos + n {
                        let defs = s.types.as_ref()?;
                        return defs.get(usize::try_from(want - pos).ok()?);
                    }
                    pos += n;
                }
                10 => {
                    for import in s.imports.iter().flatten() {
                        if let ExternDesc::Type(_) = import.desc {
                            if pos == want {
                                return None;
                            }
                            pos += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
}

/// An index space of a component. Each sort of import or export names an entry
/// in one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// Core modules.
    CoreModule,
    /// Core types.
    CoreType,
    /// Core instances.
    CoreInstance,
    /// Core functions.
    CoreFunc,
    /// Component functions.
    Func,
    /// Values.
    Value,
    /// Component types.
    Type,
    /// Component instances.
    Instance,
    /// Components.
    Component,
}

/// The kind of a component-level import or export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternKind {
    /// A core module.
    CoreModule,
    /// A function.
    Func,
    /// A value.
    Value,
    /// A type.
    Type,
    /// An instance.
    Instance,
    /// A component.
    Component,
}

impl ExternKind {
    /// The index space an export of this kind names an entry in.
    fn space(self) -> Space {
        match self {
            ExternKind::CoreModule => Space::CoreModule,
            ExternKind::Func => Space::Func,
            ExternKind::Value => Space::Value,
            ExternKind::Type => Space::Type,
            ExternKind::Instance => Space::Instance,
            ExternKind::Component => Space::Component,
        }
    }
}

/// The bound a type import places on the type it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeBound {
    /// The type is equal to the type at this index.
    Eq(u32),
    /// The type is a resource whose identity is new.
    SubResource,
}

/// The descriptor of an import: what it needs. An export may also carry one,
/// which gives the type the export is declared with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternDesc {
    /// A core module of the core type at this index.
    CoreModule(u32),
    /// A function of the component type at this index.
    Func(u32),
    /// A value of this type.
    Value(ValueType),
    /// A type, bounded as given.
    Type(TypeBound),
    /// An instance of the component type at this index.
    Instance(u32),
    /// A component of the component type at this index.
    Component(u32),
}

impl ExternDesc {
    /// The kind this descriptor describes.
    pub fn kind(&self) -> ExternKind {
        match self {
            ExternDesc::CoreModule(_) => ExternKind::CoreModule,
            ExternDesc::Func(_) => ExternKind::Func,
            ExternDesc::Value(_) => ExternKind::Value,
            ExternDesc::Type(_) => ExternKind::Type,
            ExternDesc::Instance(_) => ExternKind::Instance,
            ExternDesc::Component(_) => ExternKind::Component,
        }
    }
}

/// One entry of a component's import section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentImport {
    /// The name the import is known by.
    pub name: String,
    /// What the import needs.
    pub desc: ExternDesc,
}

/// One entry of a component's export section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentExport {
    /// The name the export is known by.
    pub name: String,
    /// The kind of entity exported.
    pub kind: ExternKind,
    /// The index of the exported entity in its index space.
    pub index: u32,
    /// The declared type of the export, if the binary gives one.
    pub desc: Option<ExternDesc>,
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
    /// For import sections (id 10), the imports, in order.
    pub imports: Option<Vec<ComponentImport>>,
    /// For export sections (id 11), the exports, in order.
    pub exports: Option<Vec<ComponentExport>>,
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
            imports: None,
            exports: None,
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
            10 => {
                let mut s = Reader {
                    bytes: body,
                    pos: 0,
                    base: body_base,
                };
                let n = s.count()?;
                section.count = Some(n);
                let mut imports = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    imports.push(import(&mut s)?);
                }
                if !s.done() {
                    return Err(s.err(ErrorKind::TrailingBytes));
                }
                section.imports = Some(imports);
            }
            11 => {
                let mut s = Reader {
                    bytes: body,
                    pos: 0,
                    base: body_base,
                };
                let n = s.count()?;
                section.count = Some(n);
                let mut exports = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    exports.push(export(&mut s)?);
                }
                if !s.done() {
                    return Err(s.err(ErrorKind::TrailingBytes));
                }
                section.exports = Some(exports);
            }
            2 | 3 | 5 | 6 | 8 | 12 => {
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
    let bounds = Bounds::of(&sections);
    for s in &sections {
        for import in s.imports.iter().flatten() {
            check_desc(&bounds, &import.desc).map_err(|k| err(s.offset, k))?;
        }
        for export in s.exports.iter().flatten() {
            let space = export.kind.space();
            check_index(&bounds, space, export.index).map_err(|k| err(s.offset, k))?;
            if let Some(desc) = &export.desc {
                check_desc(&bounds, desc).map_err(|k| err(s.offset, k))?;
            }
        }
    }
    check_names(&sections)?;
    Ok(Component { sections })
}

/// Reads an extern name: a 0x00 flag, then a length-prefixed UTF-8 name.
fn extern_name(r: &mut Reader<'_>) -> Result<String, ParseError> {
    match r.byte()? {
        0x00 => r.name(),
        other => Err(r.err(ErrorKind::BadExternName(other))),
    }
}

/// Reads an optional-field flag, which is 0x00 for none and 0x01 for some.
fn option_flag(r: &mut Reader<'_>) -> Result<bool, ParseError> {
    match r.byte()? {
        0x00 => Ok(false),
        0x01 => Ok(true),
        other => Err(r.err(ErrorKind::BadOptionFlag(other))),
    }
}

/// Reads a type bound: 0x00 and a type index, or 0x01 for a sub resource.
fn type_bound(r: &mut Reader<'_>) -> Result<TypeBound, ParseError> {
    match r.byte()? {
        0x00 => Ok(TypeBound::Eq(r.u32()?)),
        0x01 => Ok(TypeBound::SubResource),
        other => Err(r.err(ErrorKind::BadTypeBound(other))),
    }
}

/// Reads an extern descriptor. A core module is 0x00 0x11 and a type index.
fn extern_desc(r: &mut Reader<'_>) -> Result<ExternDesc, ParseError> {
    let tag = r.byte()?;
    Ok(match tag {
        0x00 => match r.byte()? {
            0x11 => ExternDesc::CoreModule(r.u32()?),
            other => return Err(r.err(ErrorKind::BadExternKind(other))),
        },
        0x01 => ExternDesc::Func(r.u32()?),
        0x02 => ExternDesc::Value(types::value_type(r)?),
        0x03 => ExternDesc::Type(type_bound(r)?),
        0x04 => ExternDesc::Component(r.u32()?),
        0x05 => ExternDesc::Instance(r.u32()?),
        other => return Err(r.err(ErrorKind::BadExternKind(other))),
    })
}

/// Reads one import: a name, then its descriptor.
fn import(r: &mut Reader<'_>) -> Result<ComponentImport, ParseError> {
    let name = extern_name(r)?;
    let desc = extern_desc(r)?;
    Ok(ComponentImport { name, desc })
}

/// Reads one export: a name, a sort and an index, then an optional descriptor.
fn export(r: &mut Reader<'_>) -> Result<ComponentExport, ParseError> {
    let name = extern_name(r)?;
    let kind = match r.byte()? {
        0x00 => match r.byte()? {
            0x11 => ExternKind::CoreModule,
            other => return Err(r.err(ErrorKind::UnsupportedSort(other))),
        },
        0x01 => ExternKind::Func,
        0x02 => ExternKind::Value,
        0x03 => ExternKind::Type,
        0x04 => ExternKind::Component,
        0x05 => ExternKind::Instance,
        other => return Err(r.err(ErrorKind::BadSort(other))),
    };
    let index = r.u32()?;
    let desc = if option_flag(r)? {
        let d = extern_desc(r)?;
        if d.kind() != kind {
            return Err(r.err(ErrorKind::DescKindMismatch));
        }
        Some(d)
    } else {
        None
    };
    Ok(ComponentExport {
        name,
        kind,
        index,
        desc,
    })
}

/// Upper bounds on the size of each index space, as the sections define it.
struct Bounds {
    core_module: u64,
    core_type: u64,
    core_instance: u64,
    core_func: u64,
    func: u64,
    value: u64,
    ty: u64,
    instance: u64,
    component: u64,
}

impl Bounds {
    /// Counts what each section can add to each space. An alias may add an
    /// entry to any space, so the alias count is added to every space.
    fn of(sections: &[Section]) -> Self {
        let mut b = Bounds {
            core_module: 0,
            core_type: 0,
            core_instance: 0,
            core_func: 0,
            func: 0,
            value: 0,
            ty: 0,
            instance: 0,
            component: 0,
        };
        let mut aliases = 0u64;
        for s in sections {
            // A core module and a nested component are one entry per section.
            let n = match s.id {
                1 | 4 => 1,
                0 | 9 => 0,
                _ => u64::from(s.count.unwrap_or(0)),
            };
            match s.id {
                1 => b.core_module += n,
                2 => b.core_instance += n,
                3 => b.core_type += n,
                4 => b.component += n,
                5 => b.instance += n,
                6 => aliases += n,
                7 => b.ty += n,
                // Each canonical function is a lifted function or a lowered core function.
                8 => {
                    b.func += n;
                    b.core_func += n;
                }
                12 => b.value += n,
                _ => {}
            }
            for import in s.imports.iter().flatten() {
                match import.desc.kind() {
                    ExternKind::CoreModule => b.core_module += 1,
                    ExternKind::Func => b.func += 1,
                    ExternKind::Value => b.value += 1,
                    ExternKind::Type => b.ty += 1,
                    ExternKind::Instance => b.instance += 1,
                    ExternKind::Component => b.component += 1,
                }
            }
        }
        b.core_module += aliases;
        b.core_type += aliases;
        b.core_instance += aliases;
        b.core_func += aliases;
        b.func += aliases;
        b.value += aliases;
        b.ty += aliases;
        b.instance += aliases;
        b.component += aliases;
        b
    }

    fn limit(&self, space: Space) -> u64 {
        match space {
            Space::CoreModule => self.core_module,
            Space::CoreType => self.core_type,
            Space::CoreInstance => self.core_instance,
            Space::CoreFunc => self.core_func,
            Space::Func => self.func,
            Space::Value => self.value,
            Space::Type => self.ty,
            Space::Instance => self.instance,
            Space::Component => self.component,
        }
    }
}

fn check_index(bounds: &Bounds, space: Space, index: u32) -> Result<(), ErrorKind> {
    if u64::from(index) < bounds.limit(space) {
        Ok(())
    } else {
        Err(ErrorKind::BadComponentIndex(space, index))
    }
}

/// Checks the type index a descriptor names, against the space it names.
fn check_desc(bounds: &Bounds, desc: &ExternDesc) -> Result<(), ErrorKind> {
    match desc {
        ExternDesc::CoreModule(i) => check_index(bounds, Space::CoreType, *i),
        ExternDesc::Func(i) | ExternDesc::Instance(i) | ExternDesc::Component(i) => {
            check_index(bounds, Space::Type, *i)
        }
        ExternDesc::Value(ValueType::Index(i)) => check_index(bounds, Space::Type, *i),
        ExternDesc::Value(ValueType::Primitive(_)) => Ok(()),
        ExternDesc::Type(TypeBound::Eq(i)) => check_index(bounds, Space::Type, *i),
        ExternDesc::Type(TypeBound::SubResource) => Ok(()),
    }
}

/// Refuses two imports with one name, and two exports with one name.
fn check_names(sections: &[Section]) -> Result<(), ParseError> {
    let mut imports = BTreeSet::new();
    let mut exports = BTreeSet::new();
    for s in sections {
        for import in s.imports.iter().flatten() {
            if !imports.insert(import.name.as_str()) {
                return Err(err(s.offset, ErrorKind::DuplicateImport));
            }
        }
        for export in s.exports.iter().flatten() {
            if !exports.insert(export.name.as_str()) {
                return Err(err(s.offset, ErrorKind::DuplicateExport));
            }
        }
    }
    Ok(())
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

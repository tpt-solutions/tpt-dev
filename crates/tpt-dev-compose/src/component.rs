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
//! Aliases (section 6) are decoded too.
//!
//! The canonical function section (8) is not decoded: its entries have variable
//! length, and only the number of entries is known. Section order is not
//! enforced, since components may place sections in any order.
//!
//! Index bounds are exact for every space except the two function spaces, which
//! the canonical section also feeds. An index is checked against the entries
//! defined before it, in section order, so a reference to a later definition is
//! refused. Inside component and instance types, indices are checked against the
//! type's own declarations. Outer aliases, which name an enclosing scope, are not
//! bounds-checked, since that scope is not tracked here.

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

use crate::compare::{Entry, top_entries};
use crate::module::{self, ErrorKind, Module, ParseError};
use crate::types::{self, Decl, TypeDef, ValueType};

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
    /// The type at `index` in the component's type index space, if it is a
    /// type this parser decodes.
    ///
    /// Returns `None` when the index is out of range, when it names a type that
    /// is not decoded, or when it names an import or alias of a type, since
    /// those are not resolved here.
    pub fn type_def(&self, index: u32) -> Option<&TypeDef> {
        let entries = top_entries(&self.sections);
        match entries.get(index as usize)? {
            Entry::Def(def) => Some(def),
            Entry::Unknown => None,
        }
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

/// The kind of a core export that a core-instance alias names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreExportKind {
    /// A core function.
    Func,
    /// A table.
    Table,
    /// A memory.
    Memory,
    /// A global.
    Global,
    /// A tag.
    Tag,
}

/// The kind of an item in an enclosing scope that an outer alias names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OuterKind {
    /// A core type.
    CoreType,
    /// A core module.
    CoreModule,
    /// A component type.
    Type,
    /// A component.
    Component,
}

/// An alias: it brings an entry from another index space into the space it
/// appears in. Each one adds exactly one entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Alias {
    /// An export of a component instance.
    InstanceExport {
        /// The kind of the export.
        kind: ExternKind,
        /// The instance index.
        instance: u32,
        /// The export name.
        name: String,
    },
    /// An export of a core instance.
    CoreInstanceExport {
        /// The kind of the export.
        kind: CoreExportKind,
        /// The core instance index.
        instance: u32,
        /// The export name.
        name: String,
    },
    /// An item of an enclosing scope, `count` scopes out.
    Outer {
        /// The kind of the item.
        kind: OuterKind,
        /// How many scopes out the item is.
        count: u32,
        /// The index of the item in that scope.
        index: u32,
    },
}

impl Alias {
    /// The index space the alias adds an entry to. Core tables, memories, globals
    /// and tags are not tracked here, so an alias of one of those returns `None`.
    pub fn space(&self) -> Option<Space> {
        match self {
            Alias::InstanceExport { kind, .. } => Some(kind.space()),
            Alias::CoreInstanceExport { kind, .. } => match kind {
                CoreExportKind::Func => Some(Space::CoreFunc),
                _ => None,
            },
            Alias::Outer { kind, .. } => Some(match kind {
                OuterKind::CoreType => Space::CoreType,
                OuterKind::CoreModule => Space::CoreModule,
                OuterKind::Type => Space::Type,
                OuterKind::Component => Space::Component,
            }),
        }
    }
}

impl ExternKind {
    /// The index space an export of this kind names an entry in.
    pub(crate) fn space(self) -> Space {
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
    /// For alias sections (id 6), the aliases, in order.
    pub aliases: Option<Vec<Alias>>,
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
            aliases: None,
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
            6 => {
                let mut s = Reader {
                    bytes: body,
                    pos: 0,
                    base: body_base,
                };
                let n = s.count()?;
                section.count = Some(n);
                let mut aliases = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    aliases.push(alias(&mut s)?);
                }
                if !s.done() {
                    return Err(s.err(ErrorKind::TrailingBytes));
                }
                section.aliases = Some(aliases);
            }
            2 | 3 | 5 | 8 | 12 => {
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
    check_sections(&sections)?;
    check_names(&sections)?;
    Ok(Component { sections })
}

/// Reads an alias: its sort, then its target. The sort is one byte, or 0x00
/// and a second byte for the sorts that need one.
pub(crate) fn alias(r: &mut Reader<'_>) -> Result<Alias, ParseError> {
    let first = r.byte()?;
    let second = if first == 0x00 { Some(r.byte()?) } else { None };
    let target = r.byte()?;
    match target {
        // An export of a component instance.
        0x00 => {
            let kind = match (first, second) {
                (0x00, Some(0x11)) => ExternKind::CoreModule,
                (0x01, None) => ExternKind::Func,
                (0x02, None) => ExternKind::Value,
                (0x03, None) => ExternKind::Type,
                (0x04, None) => ExternKind::Component,
                (0x05, None) => ExternKind::Instance,
                _ => return Err(r.err(ErrorKind::BadSort(first))),
            };
            let instance = r.u32()?;
            let name = r.name()?;
            Ok(Alias::InstanceExport {
                kind,
                instance,
                name,
            })
        }
        // An export of a core instance, which only accepts a core external kind.
        0x01 => {
            let kind = match (first, second) {
                (0x00, Some(0x00)) => CoreExportKind::Func,
                (0x00, Some(0x01)) => CoreExportKind::Table,
                (0x00, Some(0x02)) => CoreExportKind::Memory,
                (0x00, Some(0x03)) => CoreExportKind::Global,
                (0x00, Some(0x04)) => CoreExportKind::Tag,
                _ => return Err(r.err(ErrorKind::BadSort(first))),
            };
            let instance = r.u32()?;
            let name = r.name()?;
            Ok(Alias::CoreInstanceExport {
                kind,
                instance,
                name,
            })
        }
        // An item of an enclosing scope.
        0x02 => {
            let kind = match (first, second) {
                (0x00, Some(0x10)) => OuterKind::CoreType,
                (0x00, Some(0x11)) => OuterKind::CoreModule,
                (0x03, None) => OuterKind::Type,
                (0x04, None) => OuterKind::Component,
                _ => return Err(r.err(ErrorKind::BadSort(first))),
            };
            let count = r.u32()?;
            let index = r.u32()?;
            Ok(Alias::Outer { kind, count, index })
        }
        other => Err(r.err(ErrorKind::BadTypeEncoding(other))),
    }
}

/// Reads an extern name: a 0x00 flag, then a length-prefixed UTF-8 name.
pub(crate) fn extern_name(r: &mut Reader<'_>) -> Result<String, ParseError> {
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
pub(crate) fn extern_desc(r: &mut Reader<'_>) -> Result<ExternDesc, ParseError> {
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
pub(crate) fn import(r: &mut Reader<'_>) -> Result<ComponentImport, ParseError> {
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

/// The number of entries each index space holds, as the sections defined so far.
#[derive(Default)]
struct Counts {
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

impl Counts {
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

    fn bump(&mut self, space: Space, n: u64) {
        let slot = match space {
            Space::CoreModule => &mut self.core_module,
            Space::CoreType => &mut self.core_type,
            Space::CoreInstance => &mut self.core_instance,
            Space::CoreFunc => &mut self.core_func,
            Space::Func => &mut self.func,
            Space::Value => &mut self.value,
            Space::Type => &mut self.ty,
            Space::Instance => &mut self.instance,
            Space::Component => &mut self.component,
        };
        *slot = slot.saturating_add(n);
    }
}

/// Walks the sections in order, checking each index against the entries defined
/// before it. A canonical function section adds to both function spaces, since
/// its entries are not decoded, so the bound there is an upper one.
fn check_sections(sections: &[Section]) -> Result<(), ParseError> {
    let mut counts = Counts::default();
    for s in sections {
        let at = |kind| err(s.offset, kind);
        let n = u64::from(s.count.unwrap_or(0));
        match s.id {
            1 => counts.bump(Space::CoreModule, 1),
            2 => counts.bump(Space::CoreInstance, n),
            3 => counts.bump(Space::CoreType, n),
            4 => counts.bump(Space::Component, 1),
            5 => counts.bump(Space::Instance, n),
            6 => {
                for alias in s.aliases.iter().flatten() {
                    if let Some(space) = alias.space() {
                        counts.bump(space, 1);
                    }
                }
            }
            7 => match &s.types {
                Some(defs) => {
                    for def in defs {
                        check_def(def, &counts).map_err(at)?;
                        counts.bump(Space::Type, 1);
                    }
                }
                None => counts.bump(Space::Type, n),
            },
            8 => {
                counts.bump(Space::Func, n);
                counts.bump(Space::CoreFunc, n);
            }
            10 => {
                for import in s.imports.iter().flatten() {
                    check_desc(&counts, &import.desc).map_err(at)?;
                    counts.bump(import.desc.kind().space(), 1);
                }
            }
            11 => {
                for export in s.exports.iter().flatten() {
                    check_index(&counts, export.kind.space(), export.index).map_err(at)?;
                    if let Some(desc) = &export.desc {
                        check_desc(&counts, desc).map_err(at)?;
                    }
                }
            }
            12 => counts.bump(Space::Value, n),
            _ => {}
        }
    }
    Ok(())
}

/// Checks that an index is inside the space it names, given the entries so far.
fn check_index(counts: &Counts, space: Space, index: u32) -> Result<(), ErrorKind> {
    if u64::from(index) < counts.limit(space) {
        Ok(())
    } else {
        Err(ErrorKind::BadComponentIndex(space, index))
    }
}

/// Checks the type index a descriptor names, against the space it names.
fn check_desc(counts: &Counts, desc: &ExternDesc) -> Result<(), ErrorKind> {
    match desc {
        ExternDesc::CoreModule(i) => check_index(counts, Space::CoreType, *i),
        ExternDesc::Func(i) | ExternDesc::Instance(i) | ExternDesc::Component(i) => {
            check_index(counts, Space::Type, *i)
        }
        ExternDesc::Value(ValueType::Index(i)) => check_index(counts, Space::Type, *i),
        ExternDesc::Value(ValueType::Primitive(_)) => Ok(()),
        ExternDesc::Type(TypeBound::Eq(i)) => check_index(counts, Space::Type, *i),
        ExternDesc::Type(TypeBound::SubResource) => Ok(()),
    }
}

/// Checks the type references in a defined type. `counts` holds the entries
/// defined before this type, so every reference must name one of them. A
/// component or instance type is checked against its own declarations.
fn check_def(def: &TypeDef, counts: &Counts) -> Result<(), ErrorKind> {
    let value = |v: &ValueType| match v {
        ValueType::Primitive(_) => Ok(()),
        ValueType::Index(i) => check_index(counts, Space::Type, *i),
    };
    let opt = |v: &Option<ValueType>| v.as_ref().map_or(Ok(()), value);
    match def {
        TypeDef::Primitive(_) | TypeDef::Flags(_) | TypeDef::Enum(_) => Ok(()),
        TypeDef::Resource { .. } => Ok(()),
        TypeDef::Record(fields) => fields.iter().try_for_each(|f| value(&f.ty)),
        TypeDef::Variant(cases) => cases.iter().try_for_each(|c| opt(&c.ty)),
        TypeDef::List(t) | TypeDef::Option(t) => value(t),
        TypeDef::Tuple(ts) => ts.iter().try_for_each(value),
        TypeDef::Result { ok, err } => opt(ok).and(opt(err)),
        TypeDef::Own(i) | TypeDef::Borrow(i) => check_index(counts, Space::Type, *i),
        TypeDef::Func(ft) => ft
            .params
            .iter()
            .chain(ft.results.iter())
            .try_for_each(|f| value(&f.ty)),
        TypeDef::Component(decls) | TypeDef::Instance(decls) => check_decls(decls),
    }
}

/// Checks the declarations of a component or instance type, in order, each
/// against the entries its own declarations have defined before it.
fn check_decls(decls: &[Decl]) -> Result<(), ErrorKind> {
    let mut local = Counts::default();
    for decl in decls {
        match decl {
            Decl::CoreType(_) => local.bump(Space::CoreType, 1),
            Decl::Type(def) => {
                check_def(def, &local)?;
                local.bump(Space::Type, 1);
            }
            Decl::Alias(alias) => {
                if let Some(space) = alias.space() {
                    local.bump(space, 1);
                }
            }
            Decl::Import(import) => {
                check_desc(&local, &import.desc)?;
                local.bump(import.desc.kind().space(), 1);
            }
            // An export in a type defines an entry in the space of its kind, as
            // an import does. A top-level export defines nothing.
            Decl::Export { desc, .. } => {
                check_desc(&local, desc)?;
                local.bump(desc.kind().space(), 1);
            }
        }
    }
    Ok(())
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

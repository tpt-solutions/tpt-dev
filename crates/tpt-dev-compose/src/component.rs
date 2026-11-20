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
//! The canonical function section (8) is decoded for `canon lift`, `canon lower`
//! and the three resource built-ins. Other built-ins and the async, callback,
//! core-type and gc options are refused, since they are not modelled. Section
//! order is not enforced, since components may place sections in any order.
//!
//! Index bounds are exact for every space. An index is checked against the
//! entries defined before it, in section order, so a reference to a later
//! definition is refused. Each component and each component or module type is a
//! scope. An outer alias names a scope that encloses it, and is checked against
//! that scope's entries, so a count beyond the outermost scope is refused. The
//! memory option of a canonical entry is not checked, since core memories are not
//! tracked.

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

use crate::compare::{Entry, top_entries};
use crate::module::{self, ErrorKind, Module, ParseError};
use crate::types::{self, CoreType, Decl, ModuleDecl, TypeDef, ValueType};

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
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::parse_component;
    ///
    /// const IMPORTS: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/component/imports.wasm"));
    ///
    /// let component = parse_component(IMPORTS).unwrap();
    /// assert!(component.imports().count() > 0);
    /// ```
    pub fn imports(&self) -> impl Iterator<Item = &ComponentImport> {
        self.sections
            .iter()
            .filter_map(|s| s.imports.as_deref())
            .flatten()
    }

    /// Every export of the component, in the order the sections give them.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::parse_component;
    ///
    /// const ADD: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/component/add.wasm"));
    ///
    /// let component = parse_component(ADD).unwrap();
    /// // The `add` fixture exports one item.
    /// assert_eq!(component.exports().count(), 1);
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::parse_component;
    ///
    /// const ADD: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/component/add.wasm"));
    ///
    /// let component = parse_component(ADD).unwrap();
    /// assert!(component.type_def(0).is_some());
    /// // Past the last type there is nothing.
    /// assert!(component.type_def(99).is_none());
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::parse_component;
    ///
    /// const ADD: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/component/add.wasm"));
    ///
    /// let component = parse_component(ADD).unwrap();
    /// // Section 2 of the fixture is the alias section.
    /// let alias = &component.sections[2].aliases.as_ref().unwrap()[0];
    /// assert!(alias.space().is_some());
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::{ExternDesc, ExternKind};
    ///
    /// assert_eq!(ExternDesc::Func(0).kind(), ExternKind::Func);
    /// ```
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

/// A canonical option of a `canon lift` or `canon lower`.
///
/// Options that change how a function is called, such as `async`, `callback`,
/// `core-type` and `gc`, are refused with [`ErrorKind::UnsupportedCanonOption`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonOption {
    /// Strings are UTF-8.
    Utf8,
    /// Strings are UTF-16.
    Utf16,
    /// Strings are Latin-1 or UTF-16, whichever the value takes.
    CompactUtf16,
    /// The core memory at this index.
    Memory(u32),
    /// The core function at this index, which allocates memory for the callee.
    Realloc(u32),
    /// The core function at this index, which runs after the callee returns.
    PostReturn(u32),
}

/// One entry of a canonical function section.
///
/// The built-ins other than the ones here, such as the task, stream, future,
/// thread and waitable intrinsics, are refused with
/// [`ErrorKind::UnsupportedCanon`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Canon {
    /// `canon lift`: a component function, made from the core function at
    /// `core_func`, whose type is the component type at `type_index`. It adds a
    /// component function.
    Lift {
        /// The core function that implements the component function.
        core_func: u32,
        /// The options it is lifted with.
        options: Vec<CanonOption>,
        /// The component function type.
        type_index: u32,
    },
    /// `canon lower`: a core function that calls the component function `func`.
    /// It adds a core function.
    Lower {
        /// The component function being lowered.
        func: u32,
        /// The options it is lowered with.
        options: Vec<CanonOption>,
    },
    /// `canon resource.new`: a core function that makes a handle to a new
    /// resource of the given type. It adds a core function.
    ResourceNew(u32),
    /// `canon resource.drop`: a core function that drops a handle of the given
    /// resource type. It adds a core function.
    ResourceDrop(u32),
    /// `canon resource.rep`: a core function that gives the representation of a
    /// handle of the given resource type. It adds a core function.
    ResourceRep(u32),
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
    /// For canonical function sections (id 8), the entries, in order.
    pub canons: Option<Vec<Canon>>,
}

impl Section {
    /// The name the specification gives to a section id.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::component::Section;
    ///
    /// assert_eq!(Section::kind_name(11), "export");
    /// assert_eq!(Section::kind_name(200), "unknown");
    /// ```
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
///
/// # Examples
///
/// ```
/// use tpt_dev_compose::parse_component;
///
/// const ADD: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/component/add.wasm"));
///
/// let component = parse_component(ADD).unwrap();
/// assert_eq!(component.sections.len(), 7);
/// // Bytes that are not a component are refused.
/// assert!(parse_component(b"not wasm").is_err());
/// ```
pub fn parse(bytes: &[u8]) -> Result<Component, ParseError> {
    parse_at(bytes, 0, 0, Vec::new())
}

fn err(offset: usize, kind: ErrorKind) -> ParseError {
    ParseError { offset, kind }
}

fn parse_at(
    bytes: &[u8],
    base: usize,
    depth: usize,
    enclosing: Vec<Counts>,
) -> Result<Component, ParseError> {
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
    let mut scopes = Scopes::new(enclosing);
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
            canons: None,
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
                section.nested = Some(parse_at(body, body_base, depth + 1, scopes.snapshot())?);
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
            8 => {
                let mut s = Reader {
                    bytes: body,
                    pos: 0,
                    base: body_base,
                };
                let n = s.count()?;
                section.count = Some(n);
                let mut canons = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    canons.push(canon(&mut s)?);
                }
                if !s.done() {
                    return Err(s.err(ErrorKind::TrailingBytes));
                }
                section.canons = Some(canons);
            }
            2 | 3 | 5 | 12 => {
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
        check_section(&section, &mut scopes)?;
        sections.push(section);
    }
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

/// Reads a canonical function entry: an opcode, then its immediates.
pub(crate) fn canon(r: &mut Reader<'_>) -> Result<Canon, ParseError> {
    let op = r.byte()?;
    Ok(match op {
        // canon lift: a 0x00 sub-opcode, the core function, the options, the type.
        0x00 => {
            expect_zero(r)?;
            let core_func = r.u32()?;
            let options = canon_options(r)?;
            let type_index = r.u32()?;
            Canon::Lift {
                core_func,
                options,
                type_index,
            }
        }
        // canon lower: a 0x00 sub-opcode, the component function, the options.
        0x01 => {
            expect_zero(r)?;
            let func = r.u32()?;
            let options = canon_options(r)?;
            Canon::Lower { func, options }
        }
        0x02 => Canon::ResourceNew(r.u32()?),
        0x03 => Canon::ResourceDrop(r.u32()?),
        0x04 => Canon::ResourceRep(r.u32()?),
        other => return Err(r.err(ErrorKind::UnsupportedCanon(other))),
    })
}

/// Reads the 0x00 sub-opcode that the lift and lower entries carry.
fn expect_zero(r: &mut Reader<'_>) -> Result<(), ParseError> {
    match r.byte()? {
        0x00 => Ok(()),
        other => Err(r.err(ErrorKind::BadTypeEncoding(other))),
    }
}

/// Reads a vector of canonical options.
fn canon_options(r: &mut Reader<'_>) -> Result<Vec<CanonOption>, ParseError> {
    let n = r.count()?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let opt = match r.byte()? {
            0x00 => CanonOption::Utf8,
            0x01 => CanonOption::Utf16,
            0x02 => CanonOption::CompactUtf16,
            0x03 => CanonOption::Memory(r.u32()?),
            0x04 => CanonOption::Realloc(r.u32()?),
            0x05 => CanonOption::PostReturn(r.u32()?),
            code @ 0x06..=0x09 => return Err(r.err(ErrorKind::UnsupportedCanonOption(code))),
            other => return Err(r.err(ErrorKind::BadTypeEncoding(other))),
        };
        out.push(opt);
    }
    Ok(out)
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
#[derive(Clone, Default)]
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

/// The index spaces of the scope being checked, and of the scopes around it.
///
/// A component and each component or module type is a scope. An outer alias
/// with count `n` names the scope `n` levels out, where zero is the scope being
/// checked. `enclosing` is ordered outermost first, so its last entry is the
/// nearest enclosing scope.
struct Scopes {
    enclosing: Vec<Counts>,
    local: Counts,
}

impl Scopes {
    fn new(enclosing: Vec<Counts>) -> Self {
        Self {
            enclosing,
            local: Counts::default(),
        }
    }

    /// The scopes a component nested in this one sees: the enclosing scopes,
    /// then this one as it stands now.
    fn snapshot(&self) -> Vec<Counts> {
        let mut all = self.enclosing.clone();
        all.push(self.local.clone());
        all
    }

    /// Starts a nested scope. The current scope becomes enclosing.
    fn enter(&mut self) {
        let parent = core::mem::take(&mut self.local);
        self.enclosing.push(parent);
    }

    /// Ends the nested scope that `enter` started, and restores the parent.
    fn leave(&mut self) {
        if let Some(parent) = self.enclosing.pop() {
            self.local = parent;
        }
    }

    /// The scope `count` levels out, or an error if there is no such scope.
    fn outer(&self, count: u32) -> Result<&Counts, ErrorKind> {
        if count == 0 {
            return Ok(&self.local);
        }
        self.enclosing
            .len()
            .checked_sub(count as usize)
            .and_then(|i| self.enclosing.get(i))
            .ok_or(ErrorKind::BadOuterScope(count))
    }
}

/// Checks one section against the entries defined before it, then adds the
/// entries the section defines to the scope. Each index is checked as the
/// sections are read, so a reference to a later definition is refused.
fn check_section(s: &Section, scopes: &mut Scopes) -> Result<(), ParseError> {
    let at = |kind| err(s.offset, kind);
    let n = u64::from(s.count.unwrap_or(0));
    match s.id {
        1 => scopes.local.bump(Space::CoreModule, 1),
        2 => scopes.local.bump(Space::CoreInstance, n),
        3 => scopes.local.bump(Space::CoreType, n),
        4 => scopes.local.bump(Space::Component, 1),
        5 => scopes.local.bump(Space::Instance, n),
        6 => {
            for alias in s.aliases.iter().flatten() {
                check_alias(alias, scopes).map_err(at)?;
            }
        }
        7 => match &s.types {
            Some(defs) => {
                for def in defs {
                    check_def(def, scopes).map_err(at)?;
                    scopes.local.bump(Space::Type, 1);
                }
            }
            None => scopes.local.bump(Space::Type, n),
        },
        8 => {
            for canon in s.canons.iter().flatten() {
                check_canon(canon, &mut scopes.local).map_err(at)?;
            }
        }
        10 => {
            for import in s.imports.iter().flatten() {
                check_desc(&scopes.local, &import.desc).map_err(at)?;
                scopes.local.bump(import.desc.kind().space(), 1);
            }
        }
        11 => {
            for export in s.exports.iter().flatten() {
                check_index(&scopes.local, export.kind.space(), export.index).map_err(at)?;
                if let Some(desc) = &export.desc {
                    check_desc(&scopes.local, desc).map_err(at)?;
                }
            }
        }
        12 => scopes.local.bump(Space::Value, n),
        _ => {}
    }
    Ok(())
}

/// Checks an alias. An outer alias names an entry of an enclosing scope, which
/// must exist and must hold the index. The alias then adds its entry.
fn check_alias(alias: &Alias, scopes: &mut Scopes) -> Result<(), ErrorKind> {
    let Some(space) = alias.space() else {
        return Ok(());
    };
    if let Alias::Outer { count, index, .. } = alias {
        check_index(scopes.outer(*count)?, space, *index)?;
    }
    scopes.local.bump(space, 1);
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

/// Checks a canonical function entry against the entries before it, then adds
/// the function it defines: a component function for a lift, a core function
/// for anything else.
fn check_canon(canon: &Canon, counts: &mut Counts) -> Result<(), ErrorKind> {
    match canon {
        Canon::Lift {
            core_func,
            options,
            type_index,
        } => {
            check_index(counts, Space::CoreFunc, *core_func)?;
            check_index(counts, Space::Type, *type_index)?;
            check_options(counts, options)?;
            counts.bump(Space::Func, 1);
        }
        Canon::Lower { func, options } => {
            check_index(counts, Space::Func, *func)?;
            check_options(counts, options)?;
            counts.bump(Space::CoreFunc, 1);
        }
        Canon::ResourceNew(t) | Canon::ResourceDrop(t) | Canon::ResourceRep(t) => {
            check_index(counts, Space::Type, *t)?;
            counts.bump(Space::CoreFunc, 1);
        }
    }
    Ok(())
}

/// Checks the options that name core functions. A memory option names a core
/// memory, which this parser does not track, so it is not checked.
fn check_options(counts: &Counts, options: &[CanonOption]) -> Result<(), ErrorKind> {
    for option in options {
        match option {
            CanonOption::Realloc(i) | CanonOption::PostReturn(i) => {
                check_index(counts, Space::CoreFunc, *i)?;
            }
            CanonOption::Utf8
            | CanonOption::Utf16
            | CanonOption::CompactUtf16
            | CanonOption::Memory(_) => {}
        }
    }
    Ok(())
}

/// Checks a type definition. A component or instance type gets its own scope.
/// Any other definition is checked against the entries of the current scope.
fn check_def(def: &TypeDef, scopes: &mut Scopes) -> Result<(), ErrorKind> {
    match def {
        TypeDef::Component(decls) | TypeDef::Instance(decls) => check_decls(decls, scopes),
        _ => check_refs(def, &scopes.local),
    }
}

/// Checks the type references in a definition that has no scope of its own.
fn check_refs(def: &TypeDef, counts: &Counts) -> Result<(), ErrorKind> {
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
        TypeDef::Func(ft) | TypeDef::AsyncFunc(ft) => ft
            .params
            .iter()
            .chain(ft.results.iter())
            .try_for_each(|f| value(&f.ty)),
        TypeDef::Map(k, v) => value(k).and(value(v)),
        TypeDef::FixedList(t, _) => value(t),
        TypeDef::Stream(t) | TypeDef::Future(t) => opt(t),
        // Checked by `check_decls`, in the scope of their own declarations.
        TypeDef::Component(_) | TypeDef::Instance(_) => Ok(()),
    }
}

/// Checks the declarations of a component or instance type, in order, in a
/// scope of their own.
fn check_decls(decls: &[Decl], scopes: &mut Scopes) -> Result<(), ErrorKind> {
    scopes.enter();
    let result = decls.iter().try_for_each(|d| check_decl(d, scopes));
    scopes.leave();
    result
}

fn check_decl(decl: &Decl, scopes: &mut Scopes) -> Result<(), ErrorKind> {
    match decl {
        Decl::CoreType(CoreType::Func(_)) => scopes.local.bump(Space::CoreType, 1),
        Decl::CoreType(CoreType::Module(decls)) => {
            check_module(decls, scopes)?;
            scopes.local.bump(Space::CoreType, 1);
        }
        Decl::Type(def) => {
            check_def(def, scopes)?;
            scopes.local.bump(Space::Type, 1);
        }
        Decl::Alias(alias) => check_alias(alias, scopes)?,
        Decl::Import(import) => {
            check_desc(&scopes.local, &import.desc)?;
            scopes.local.bump(import.desc.kind().space(), 1);
        }
        // An export in a type defines an entry in the space of its kind, as an
        // import does. A top-level export defines nothing.
        Decl::Export { desc, .. } => {
            check_desc(&scopes.local, desc)?;
            scopes.local.bump(desc.kind().space(), 1);
        }
    }
    Ok(())
}

/// Checks the declarations of a core module type. Its type indices were checked
/// when they were read. An outer alias names a core type of an enclosing scope.
fn check_module(decls: &[ModuleDecl], scopes: &mut Scopes) -> Result<(), ErrorKind> {
    scopes.enter();
    let result = decls.iter().try_for_each(|d| match d {
        ModuleDecl::Type(_) => {
            scopes.local.bump(Space::CoreType, 1);
            Ok(())
        }
        ModuleDecl::Alias { count, index } => {
            check_index(scopes.outer(*count)?, Space::CoreType, *index)?;
            scopes.local.bump(Space::CoreType, 1);
            Ok(())
        }
        ModuleDecl::Import(_) | ModuleDecl::Export { .. } => Ok(()),
    });
    scopes.leave();
    result
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

//! Component Model types: decoding the type section, and the canonical ABI's
//! size, alignment and flattening for value types.
//!
//! [`decode_items`] reads the type section of a component: the defined value
//! types, function types, resource types, and the component and instance types
//! with their declarations. Module types, async function types, and the value
//! types this parser does not model (streams, futures, error contexts, fixed-size
//! lists, maps) are not decoded, so a section that holds one yields `None`.
//! Malformed bytes are a [`ParseError`], never a guess.
//!
//! A type reference is an index into the component's whole type index space,
//! which also includes types from imports, aliases and earlier sections. This
//! module keeps the index as written. [`layout`] and [`flatten`] take that
//! space as a slice, so the caller supplies the types it has, and an index
//! outside the slice is reported rather than read.
//!
//! The layout rules follow the canonical ABI of the Component Model
//! specification. The tests check them against values worked out from those
//! rules by hand.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::component::{self, ComponentImport, ExternDesc, Reader};
use crate::module::{self, ErrorKind, ParseError, ValType};

/// A primitive value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primitive {
    /// `bool`.
    Bool,
    /// `s8`.
    S8,
    /// `u8`.
    U8,
    /// `s16`.
    S16,
    /// `u16`.
    U16,
    /// `s32`.
    S32,
    /// `u32`.
    U32,
    /// `s64`.
    S64,
    /// `u64`.
    U64,
    /// `f32`.
    F32,
    /// `f64`.
    F64,
    /// `char`.
    Char,
    /// `string`.
    String,
}

/// A value type: a primitive, or a reference to a type by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    /// A primitive type.
    Primitive(Primitive),
    /// A type index in the component's type index space.
    Index(u32),
}

/// A labelled value: a record field, a function parameter or result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The label. Unnamed function results have an empty label.
    pub name: String,
    /// The type of the value.
    pub ty: ValueType,
}

/// One case of a variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// The case name.
    pub name: String,
    /// The payload type, if the case has one.
    pub ty: Option<ValueType>,
}

/// A function type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncType {
    /// Parameters, in order.
    pub params: Vec<Field>,
    /// Results, in order.
    pub results: Vec<Field>,
}

/// A declaration inside a component or instance type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decl {
    /// A core function type, declared with `core type`.
    CoreType(module::FuncType),
    /// A type definition. It adds one entry to the type index space of its
    /// enclosing type.
    Type(TypeDef),
    /// An alias. It adds one entry to the index space of its sort.
    Alias(component::Alias),
    /// An import. Only component types have these.
    Import(ComponentImport),
    /// An export, with the type it is declared with. It adds no index.
    Export {
        /// The export name.
        name: String,
        /// The declared type of the export.
        desc: ExternDesc,
    },
}

/// One entry of a component's type section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeDef {
    /// A primitive value type.
    Primitive(Primitive),
    /// A record, with its named fields in order.
    Record(Vec<Field>),
    /// A variant, with its cases in order.
    Variant(Vec<Case>),
    /// A list of one element type.
    List(ValueType),
    /// A tuple of value types.
    Tuple(Vec<ValueType>),
    /// A set of named bit flags.
    Flags(Vec<String>),
    /// An enumeration of named cases with no payloads.
    Enum(Vec<String>),
    /// An optional value.
    Option(ValueType),
    /// A success or an error, each with an optional payload.
    Result {
        /// The success payload, if any.
        ok: Option<ValueType>,
        /// The error payload, if any.
        err: Option<ValueType>,
    },
    /// An owned handle to the resource at the given type index.
    Own(u32),
    /// A borrowed handle to the resource at the given type index.
    Borrow(u32),
    /// A function type.
    Func(FuncType),
    /// A fresh resource type: its core representation, and an optional
    /// destructor given as a function index.
    Resource {
        /// The core representation.
        rep: ValType,
        /// The destructor's function index, if one is given.
        dtor: Option<u32>,
    },
    /// A component type, with its declarations in order.
    Component(Vec<Decl>),
    /// An instance type, with its declarations in order.
    Instance(Vec<Decl>),
}

/// The size and alignment of a value in linear memory, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Size in bytes, a multiple of `align`.
    pub size: usize,
    /// Alignment in bytes, a power of two.
    pub align: usize,
}

/// Why a value type has no layout or flattening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiError {
    /// A type index is outside the type index space that was given.
    BadIndex(u32),
    /// A function type was used where a value type is needed.
    NotAValueType(u32),
    /// A variant or enum has no cases.
    EmptyVariant,
    /// The type nests too deeply, or is too large to work out within the limit.
    TooComplex,
}

fn primitive(byte: u8) -> Option<Primitive> {
    Some(match byte {
        0x7F => Primitive::Bool,
        0x7E => Primitive::S8,
        0x7D => Primitive::U8,
        0x7C => Primitive::S16,
        0x7B => Primitive::U16,
        0x7A => Primitive::S32,
        0x79 => Primitive::U32,
        0x78 => Primitive::S64,
        0x77 => Primitive::U64,
        0x76 => Primitive::F32,
        0x75 => Primitive::F64,
        0x74 => Primitive::Char,
        0x73 => Primitive::String,
        _ => return None,
    })
}

// Decoding.

/// The deepest nesting of component and instance types the parser accepts.
pub(crate) const MAX_TYPE_DEPTH: usize = 16;

/// Reads `count` type entries. Returns `None` if one of them is a kind this
/// parser does not decode, since the bytes after it cannot then be located.
pub(crate) fn decode_items(
    r: &mut Reader<'_>,
    count: u32,
) -> Result<Option<Vec<TypeDef>>, ParseError> {
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        match decode_item(r, 0)? {
            Some(def) => out.push(def),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// Reads the declarations of a component type (`component` is true) or of an
/// instance type. Returns `None` if one of them is not decoded.
fn decode_decls(
    r: &mut Reader<'_>,
    depth: usize,
    component: bool,
) -> Result<Option<Vec<Decl>>, ParseError> {
    if depth >= MAX_TYPE_DEPTH {
        return Err(r.err(ErrorKind::NestingTooDeep));
    }
    let n = r.count()?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let decl = match r.byte()? {
            // A core type. Only core function types are decoded.
            0x00 => match core_functype(r)? {
                Some(f) => Decl::CoreType(f),
                None => return Ok(None),
            },
            0x01 => match decode_item(r, depth + 1)? {
                Some(def) => Decl::Type(def),
                None => return Ok(None),
            },
            0x02 => Decl::Alias(component::alias(r)?),
            0x03 if component => Decl::Import(component::import(r)?),
            0x04 => {
                let name = component::extern_name(r)?;
                let desc = component::extern_desc(r)?;
                Decl::Export { name, desc }
            }
            other => return Err(r.err(ErrorKind::BadTypeEncoding(other))),
        };
        out.push(decl);
    }
    Ok(Some(out))
}

/// Reads a core function type: `0x60`, then the parameter and result vectors.
/// Any other core type, such as a module type or a GC type, returns `None`.
fn core_functype(r: &mut Reader<'_>) -> Result<Option<module::FuncType>, ParseError> {
    if r.peek()? != 0x60 {
        return Ok(None);
    }
    r.byte()?;
    let Some(params) = core_valtypes(r)? else {
        return Ok(None);
    };
    let Some(results) = core_valtypes(r)? else {
        return Ok(None);
    };
    Ok(Some(module::FuncType { params, results }))
}

/// Reads a vector of core value types, or `None` if one is not modelled.
fn core_valtypes(r: &mut Reader<'_>) -> Result<Option<Vec<ValType>>, ParseError> {
    let n = r.count()?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        match core_valtype(r)? {
            Some(t) => out.push(t),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// Reads a core value type. The reference types with heap types return `None`.
fn core_valtype(r: &mut Reader<'_>) -> Result<Option<ValType>, ParseError> {
    Ok(Some(match r.byte()? {
        0x7F => ValType::I32,
        0x7E => ValType::I64,
        0x7D => ValType::F32,
        0x7C => ValType::F64,
        0x7B => ValType::V128,
        0x70 => ValType::FuncRef,
        0x6F => ValType::ExternRef,
        0x63 | 0x64 => return Ok(None),
        other => return Err(r.err(ErrorKind::BadValType(other))),
    }))
}

fn decode_item(r: &mut Reader<'_>, depth: usize) -> Result<Option<TypeDef>, ParseError> {
    let tag = r.byte()?;
    if let Some(p) = primitive(tag) {
        return Ok(Some(TypeDef::Primitive(p)));
    }
    let def = match tag {
        0x3F => {
            let Some(rep) = core_valtype(r)? else {
                return Ok(None);
            };
            let dtor = match r.byte()? {
                0x00 => None,
                0x01 => Some(r.u32()?),
                other => return Err(r.err(ErrorKind::BadTypeEncoding(other))),
            };
            TypeDef::Resource { rep, dtor }
        }
        0x41 => match decode_decls(r, depth, true)? {
            Some(decls) => TypeDef::Component(decls),
            None => return Ok(None),
        },
        0x42 => match decode_decls(r, depth, false)? {
            Some(decls) => TypeDef::Instance(decls),
            None => return Ok(None),
        },
        0x72 => TypeDef::Record(fields(r)?),
        0x71 => {
            let n = r.count()?;
            let mut cases = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let name = r.name()?;
                let ty = opt_value_type(r)?;
                // The refines option, which is 0x00 for none or 0x01 and a type index.
                match r.byte()? {
                    0x00 => {}
                    0x01 => {
                        r.u32()?;
                    }
                    other => return Err(r.err(ErrorKind::BadTypeEncoding(other))),
                }
                cases.push(Case { name, ty });
            }
            TypeDef::Variant(cases)
        }
        0x70 => TypeDef::List(value_type(r)?),
        0x6F => {
            let n = r.count()?;
            let mut tys = Vec::with_capacity(n as usize);
            for _ in 0..n {
                tys.push(value_type(r)?);
            }
            TypeDef::Tuple(tys)
        }
        0x6E => TypeDef::Flags(names(r)?),
        0x6D => TypeDef::Enum(names(r)?),
        0x6B => TypeDef::Option(value_type(r)?),
        0x6A => {
            let ok = opt_value_type(r)?;
            let err = opt_value_type(r)?;
            TypeDef::Result { ok, err }
        }
        0x69 => TypeDef::Own(r.u32()?),
        0x68 => TypeDef::Borrow(r.u32()?),
        0x40 => {
            let params = fields(r)?;
            let results = match r.byte()? {
                // A single unnamed result.
                0x00 => vec![Field {
                    name: String::new(),
                    ty: value_type(r)?,
                }],
                // A list of named results, which may be empty.
                0x01 => fields(r)?,
                other => return Err(r.err(ErrorKind::BadTypeEncoding(other))),
            };
            TypeDef::Func(FuncType { params, results })
        }
        // Async function types, module types, and the value types this parser
        // does not model (streams, futures, error contexts, fixed lists, maps)
        // are left undecoded.
        _ => return Ok(None),
    };
    Ok(Some(def))
}

fn fields(r: &mut Reader<'_>) -> Result<Vec<Field>, ParseError> {
    let n = r.count()?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let name = r.name()?;
        let ty = value_type(r)?;
        out.push(Field { name, ty });
    }
    Ok(out)
}

fn names(r: &mut Reader<'_>) -> Result<Vec<String>, ParseError> {
    let n = r.count()?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        out.push(r.name()?);
    }
    Ok(out)
}

/// Reads a value type. A primitive is a single byte from 0x73 to 0x7f. Any other
/// value is a non-negative signed integer that gives a type index.
pub(crate) fn value_type(r: &mut Reader<'_>) -> Result<ValueType, ParseError> {
    let b = r.peek()?;
    if let Some(p) = primitive(b) {
        r.byte()?;
        return Ok(ValueType::Primitive(p));
    }
    let v = r.s33()?;
    u32::try_from(v)
        .map(ValueType::Index)
        .map_err(|_| r.err(ErrorKind::BadValueType(b)))
}

fn opt_value_type(r: &mut Reader<'_>) -> Result<Option<ValueType>, ParseError> {
    match r.byte()? {
        0x00 => Ok(None),
        0x01 => Ok(Some(value_type(r)?)),
        other => Err(r.err(ErrorKind::BadTypeEncoding(other))),
    }
}

// The canonical ABI.

/// Work allowed for one call to [`layout`] or [`flatten`].
const BUDGET: usize = 100_000;
/// How deeply type references may nest.
const MAX_DEPTH: usize = 64;

fn round_up(n: usize, align: usize) -> Result<usize, AbiError> {
    let r = n.checked_add(align - 1).ok_or(AbiError::TooComplex)?;
    Ok(r - r % align)
}

fn prim_layout(p: Primitive) -> Layout {
    let (size, align) = match p {
        Primitive::Bool | Primitive::S8 | Primitive::U8 => (1, 1),
        Primitive::S16 | Primitive::U16 => (2, 2),
        Primitive::S32 | Primitive::U32 | Primitive::Char | Primitive::F32 => (4, 4),
        Primitive::S64 | Primitive::U64 | Primitive::F64 => (8, 8),
        // A string is a pointer and a length.
        Primitive::String => (8, 4),
    };
    Layout { size, align }
}

fn prim_flat(p: Primitive) -> Vec<ValType> {
    match p {
        Primitive::Bool
        | Primitive::S8
        | Primitive::U8
        | Primitive::S16
        | Primitive::U16
        | Primitive::S32
        | Primitive::U32
        | Primitive::Char => vec![ValType::I32],
        Primitive::S64 | Primitive::U64 => vec![ValType::I64],
        Primitive::F32 => vec![ValType::F32],
        Primitive::F64 => vec![ValType::F64],
        Primitive::String => vec![ValType::I32, ValType::I32],
    }
}

/// The core type that two flat values at the same position share in a variant.
fn join(a: ValType, b: ValType) -> ValType {
    if a == b {
        a
    } else if matches!(
        (a, b),
        (ValType::I32, ValType::F32) | (ValType::F32, ValType::I32)
    ) {
        ValType::I32
    } else {
        ValType::I64
    }
}

struct Abi<'a> {
    space: &'a [TypeDef],
    budget: usize,
}

impl<'a> Abi<'a> {
    fn enter(&mut self, depth: usize) -> Result<(), AbiError> {
        if depth > MAX_DEPTH || self.budget == 0 {
            return Err(AbiError::TooComplex);
        }
        self.budget -= 1;
        Ok(())
    }

    fn def(&self, index: u32) -> Result<&'a TypeDef, AbiError> {
        self.space
            .get(index as usize)
            .ok_or(AbiError::BadIndex(index))
    }

    fn layout_value(&mut self, ty: &ValueType, depth: usize) -> Result<Layout, AbiError> {
        match *ty {
            ValueType::Primitive(p) => Ok(prim_layout(p)),
            ValueType::Index(i) => self.layout_def(i, depth + 1),
        }
    }

    fn layout_def(&mut self, index: u32, depth: usize) -> Result<Layout, AbiError> {
        self.enter(depth)?;
        let def = self.def(index)?;
        match def {
            TypeDef::Primitive(p) => Ok(prim_layout(*p)),
            TypeDef::Record(fields) => self.aggregate_layout(fields.iter().map(|f| &f.ty), depth),
            TypeDef::Tuple(tys) => self.aggregate_layout(tys.iter(), depth),
            TypeDef::Variant(cases) => {
                let payloads: Vec<Option<&ValueType>> =
                    cases.iter().map(|c| c.ty.as_ref()).collect();
                self.variant_layout(&payloads, depth)
            }
            TypeDef::Enum(names) => self.variant_layout(&vec![None; names.len()], depth),
            TypeDef::Option(t) => self.variant_layout(&[None, Some(t)], depth),
            TypeDef::Result { ok, err } => self.variant_layout(&[ok.as_ref(), err.as_ref()], depth),
            TypeDef::List(_) => Ok(Layout { size: 8, align: 4 }),
            TypeDef::Own(_) | TypeDef::Borrow(_) => Ok(Layout { size: 4, align: 4 }),
            TypeDef::Flags(names) => Ok(flags_layout(names.len())),
            TypeDef::Func(_)
            | TypeDef::Resource { .. }
            | TypeDef::Component(_)
            | TypeDef::Instance(_) => Err(AbiError::NotAValueType(index)),
        }
    }

    /// A record or tuple: each field at its alignment, in order.
    fn aggregate_layout<'t>(
        &mut self,
        tys: impl Iterator<Item = &'t ValueType>,
        depth: usize,
    ) -> Result<Layout, AbiError> {
        Ok(self.tuple(tys, depth)?.layout)
    }

    /// Lays out values in order, each at its alignment, and gives their offsets.
    fn tuple<'t>(
        &mut self,
        tys: impl Iterator<Item = &'t ValueType>,
        depth: usize,
    ) -> Result<MemoryTuple, AbiError> {
        let mut size = 0usize;
        let mut align = 1usize;
        let mut offsets = Vec::new();
        for t in tys {
            let l = self.layout_value(t, depth)?;
            let at = round_up(size, l.align)?;
            offsets.push(at);
            size = at.checked_add(l.size).ok_or(AbiError::TooComplex)?;
            align = align.max(l.align);
        }
        Ok(MemoryTuple {
            layout: Layout {
                size: round_up(size, align)?,
                align,
            },
            offsets,
        })
    }

    /// A variant. Its discriminant comes first, then the payload, which starts
    /// at the most-aligned case's alignment.
    fn variant_layout(
        &mut self,
        payloads: &[Option<&ValueType>],
        depth: usize,
    ) -> Result<Layout, AbiError> {
        if payloads.is_empty() {
            return Err(AbiError::EmptyVariant);
        }
        let disc = discriminant_size(payloads.len());
        let mut max_align = 1usize;
        let mut max_size = 0usize;
        for p in payloads {
            if let Some(t) = *p {
                let l = self.layout_value(t, depth)?;
                max_align = max_align.max(l.align);
                max_size = max_size.max(l.size);
            }
        }
        let align = disc.max(max_align);
        let payload_at = round_up(disc, max_align)?;
        let size = round_up(
            payload_at
                .checked_add(max_size)
                .ok_or(AbiError::TooComplex)?,
            align,
        )?;
        Ok(Layout { size, align })
    }

    fn flat_value(&mut self, ty: &ValueType, depth: usize) -> Result<Vec<ValType>, AbiError> {
        match *ty {
            ValueType::Primitive(p) => Ok(prim_flat(p)),
            ValueType::Index(i) => self.flat_def(i, depth + 1),
        }
    }

    fn flat_def(&mut self, index: u32, depth: usize) -> Result<Vec<ValType>, AbiError> {
        self.enter(depth)?;
        let def = self.def(index)?;
        match def {
            TypeDef::Primitive(p) => Ok(prim_flat(*p)),
            TypeDef::Record(fields) => self.concat_flat(fields.iter().map(|f| &f.ty), depth),
            TypeDef::Tuple(tys) => self.concat_flat(tys.iter(), depth),
            TypeDef::Variant(cases) => {
                let payloads: Vec<Option<&ValueType>> =
                    cases.iter().map(|c| c.ty.as_ref()).collect();
                self.variant_flat(&payloads, depth)
            }
            TypeDef::Enum(names) => self.variant_flat(&vec![None; names.len()], depth),
            TypeDef::Option(t) => self.variant_flat(&[None, Some(t)], depth),
            TypeDef::Result { ok, err } => self.variant_flat(&[ok.as_ref(), err.as_ref()], depth),
            TypeDef::List(_) => Ok(vec![ValType::I32, ValType::I32]),
            TypeDef::Own(_) | TypeDef::Borrow(_) => Ok(vec![ValType::I32]),
            TypeDef::Flags(names) => Ok(vec![ValType::I32; names.len().div_ceil(32)]),
            TypeDef::Func(_)
            | TypeDef::Resource { .. }
            | TypeDef::Component(_)
            | TypeDef::Instance(_) => Err(AbiError::NotAValueType(index)),
        }
    }

    fn concat_flat<'t>(
        &mut self,
        tys: impl Iterator<Item = &'t ValueType>,
        depth: usize,
    ) -> Result<Vec<ValType>, AbiError> {
        let mut out = Vec::new();
        for t in tys {
            out.extend(self.flat_value(t, depth)?);
        }
        Ok(out)
    }

    /// A variant's flat form: an i32 discriminant, then the payload slots, where
    /// each slot is the join of the cases' types in that position.
    fn variant_flat(
        &mut self,
        payloads: &[Option<&ValueType>],
        depth: usize,
    ) -> Result<Vec<ValType>, AbiError> {
        if payloads.is_empty() {
            return Err(AbiError::EmptyVariant);
        }
        let mut joined: Vec<ValType> = Vec::new();
        for p in payloads {
            if let Some(t) = *p {
                for (k, v) in self.flat_value(t, depth)?.into_iter().enumerate() {
                    if k < joined.len() {
                        joined[k] = join(joined[k], v);
                    } else {
                        joined.push(v);
                    }
                }
            }
        }
        let mut out = vec![ValType::I32];
        out.extend(joined);
        Ok(out)
    }
}

/// The bytes a variant discriminant takes for `cases` cases.
fn discriminant_size(cases: usize) -> usize {
    if cases <= 1 << 8 {
        1
    } else if cases <= 1 << 16 {
        2
    } else {
        4
    }
}

/// The layout of a flags value with `labels` flags. Up to 32 flags share one
/// integer, and more take a run of 32-bit integers.
fn flags_layout(labels: usize) -> Layout {
    match labels {
        0 => Layout { size: 0, align: 1 },
        1..=8 => Layout { size: 1, align: 1 },
        9..=16 => Layout { size: 2, align: 2 },
        17..=32 => Layout { size: 4, align: 4 },
        _ => Layout {
            size: 4 * labels.div_ceil(32),
            align: 4,
        },
    }
}

/// The most flat values a function's parameters may take before they are
/// passed through memory instead.
pub const MAX_FLAT_PARAMS: usize = 16;

/// The most flat values a function's results may take before they are returned
/// through memory instead (the 1-value rule).
pub const MAX_FLAT_RESULTS: usize = 1;

/// Which side of the boundary a function's core signature is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// A lifted function: the component exports it, and core code calls it.
    /// A spilled result is returned as a pointer.
    Lift,
    /// A lowered function: core code exports it, and the component imports it
    /// through `canon lower`. A spilled result is written through a return
    /// pointer, which is an extra final parameter.
    Lower,
}

/// A tuple of values laid out in linear memory, as a spilled parameter list or
/// result is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryTuple {
    /// The size and alignment of the whole tuple.
    pub layout: Layout,
    /// The byte offset of each value, in order.
    pub offsets: Vec<usize>,
}

/// How a function's parameters and results cross the core boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncAbi {
    /// The core parameter types. A spilled parameter list is one `i32`, a
    /// pointer to the tuple. A lowered function with a spilled result adds one
    /// more `i32`, the return pointer, as its last parameter.
    pub params: Vec<ValType>,
    /// The core result types. A spilled result is one `i32` pointer when lifted,
    /// and none when lowered (the value goes through the return pointer).
    pub results: Vec<ValType>,
    /// The parameters as a tuple in memory, when they are spilled.
    pub param_memory: Option<MemoryTuple>,
    /// The results as a tuple in memory, when they are spilled.
    pub result_memory: Option<MemoryTuple>,
    /// Whether the last parameter is a return pointer (lowered, spilled result).
    pub return_pointer: bool,
}

/// The layout of a tuple of values in linear memory, in the canonical ABI's
/// record layout: each value at its own alignment, in order.
///
/// `space` is the type index space that `tys` refer to.
pub fn tuple_layout(space: &[TypeDef], tys: &[ValueType]) -> Result<MemoryTuple, AbiError> {
    let mut abi = Abi {
        space,
        budget: BUDGET,
    };
    abi.tuple(tys.iter(), 0)
}

/// How a function type is passed at the core boundary, for one direction.
///
/// The parameters are spilled when their flat form has more than
/// [`MAX_FLAT_PARAMS`] values. The results are spilled when their flat form has
/// more than [`MAX_FLAT_RESULTS`] values. A spilled list is laid out as a tuple
/// in memory, and [`FuncAbi`] gives its offsets.
///
/// `space` is the type index space that the function's types refer to.
pub fn func_abi(space: &[TypeDef], ft: &FuncType, dir: Direction) -> Result<FuncAbi, AbiError> {
    let mut abi = Abi {
        space,
        budget: BUDGET,
    };
    let param_tys = || ft.params.iter().map(|f| &f.ty);
    let result_tys = || ft.results.iter().map(|f| &f.ty);

    let mut params = abi.concat_flat(param_tys(), 0)?;
    let flat_results = abi.concat_flat(result_tys(), 0)?;

    let param_memory = if params.len() > MAX_FLAT_PARAMS {
        params = vec![ValType::I32];
        Some(abi.tuple(param_tys(), 0)?)
    } else {
        None
    };

    let mut return_pointer = false;
    let (results, result_memory) = if flat_results.len() > MAX_FLAT_RESULTS {
        let memory = abi.tuple(result_tys(), 0)?;
        match dir {
            Direction::Lift => (vec![ValType::I32], Some(memory)),
            Direction::Lower => {
                params.push(ValType::I32);
                return_pointer = true;
                (Vec::new(), Some(memory))
            }
        }
    } else {
        (flat_results, None)
    };

    Ok(FuncAbi {
        params,
        results,
        param_memory,
        result_memory,
        return_pointer,
    })
}

/// The size and alignment of a value type, in linear memory.
///
/// `space` is the type index space that `ty` refers to.
pub fn layout(space: &[TypeDef], ty: &ValueType) -> Result<Layout, AbiError> {
    let mut abi = Abi {
        space,
        budget: BUDGET,
    };
    abi.layout_value(ty, 0)
}

/// The core value types a value type is passed as, when it is passed in
/// registers rather than through memory.
///
/// `space` is the type index space that `ty` refers to.
pub fn flatten(space: &[TypeDef], ty: &ValueType) -> Result<Vec<ValType>, AbiError> {
    let mut abi = Abi {
        space,
        budget: BUDGET,
    };
    abi.flat_value(ty, 0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::component::parse;

    const P: fn(Primitive) -> ValueType = ValueType::Primitive;

    fn f(name: &str, ty: ValueType) -> Field {
        Field {
            name: name.into(),
            ty,
        }
    }

    fn lay(ty: ValueType) -> Layout {
        layout(&[], &ty).unwrap()
    }

    #[test]
    fn primitive_sizes_and_alignments() {
        let cases = [
            (Primitive::Bool, 1, 1),
            (Primitive::U8, 1, 1),
            (Primitive::S16, 2, 2),
            (Primitive::U32, 4, 4),
            (Primitive::Char, 4, 4),
            (Primitive::F32, 4, 4),
            (Primitive::S64, 8, 8),
            (Primitive::F64, 8, 8),
            (Primitive::String, 8, 4),
        ];
        for (p, size, align) in cases {
            assert_eq!(lay(P(p)), Layout { size, align }, "{p:?}");
        }
    }

    #[test]
    fn primitive_flat_types() {
        let flat = |p| flatten(&[], &P(p)).unwrap();
        assert_eq!(flat(Primitive::U8), [ValType::I32]);
        assert_eq!(flat(Primitive::Char), [ValType::I32]);
        assert_eq!(flat(Primitive::S64), [ValType::I64]);
        assert_eq!(flat(Primitive::F32), [ValType::F32]);
        assert_eq!(flat(Primitive::String), [ValType::I32, ValType::I32]);
    }

    #[test]
    fn a_record_pads_each_field_to_its_alignment() {
        // u8 at 0, u32 at 4: size 8, align 4.
        let space = [TypeDef::Record(vec![
            f("a", P(Primitive::U8)),
            f("b", P(Primitive::U32)),
        ])];
        assert_eq!(
            layout(&space, &ValueType::Index(0)).unwrap(),
            Layout { size: 8, align: 4 }
        );
        // u8 at 0, u16 at 2, u8 at 4: size 5, rounded to 6 for align 2.
        let space = [TypeDef::Record(vec![
            f("a", P(Primitive::U8)),
            f("b", P(Primitive::U16)),
            f("c", P(Primitive::U8)),
        ])];
        assert_eq!(
            layout(&space, &ValueType::Index(0)).unwrap(),
            Layout { size: 6, align: 2 }
        );
    }

    #[test]
    fn an_empty_record_is_zero_sized() {
        let space = [TypeDef::Record(vec![])];
        assert_eq!(
            layout(&space, &ValueType::Index(0)).unwrap(),
            Layout { size: 0, align: 1 }
        );
        assert_eq!(
            flatten(&space, &ValueType::Index(0)).unwrap(),
            Vec::<ValType>::new()
        );
    }

    #[test]
    fn a_tuple_of_u8_and_u16_is_four_bytes() {
        let space = [TypeDef::Tuple(vec![P(Primitive::U8), P(Primitive::U16)])];
        assert_eq!(
            layout(&space, &ValueType::Index(0)).unwrap(),
            Layout { size: 4, align: 2 }
        );
    }

    #[test]
    fn option_of_u32_is_eight_bytes_and_two_slots() {
        let space = [TypeDef::Option(P(Primitive::U32))];
        let ty = ValueType::Index(0);
        assert_eq!(layout(&space, &ty).unwrap(), Layout { size: 8, align: 4 });
        assert_eq!(flatten(&space, &ty).unwrap(), [ValType::I32, ValType::I32]);
    }

    #[test]
    fn option_of_string_places_the_payload_after_the_discriminant() {
        // Discriminant at 0, the string at 4, size 12.
        let space = [TypeDef::Option(P(Primitive::String))];
        assert_eq!(
            layout(&space, &ValueType::Index(0)).unwrap(),
            Layout { size: 12, align: 4 }
        );
        assert_eq!(
            flatten(&space, &ValueType::Index(0)).unwrap(),
            [ValType::I32, ValType::I32, ValType::I32]
        );
    }

    #[test]
    fn result_of_u32_and_u8_is_eight_bytes() {
        let space = [TypeDef::Result {
            ok: Some(P(Primitive::U32)),
            err: Some(P(Primitive::U8)),
        }];
        assert_eq!(
            layout(&space, &ValueType::Index(0)).unwrap(),
            Layout { size: 8, align: 4 }
        );
    }

    #[test]
    fn result_with_only_a_u8_error_is_two_bytes() {
        let space = [TypeDef::Result {
            ok: None,
            err: Some(P(Primitive::U8)),
        }];
        assert_eq!(
            layout(&space, &ValueType::Index(0)).unwrap(),
            Layout { size: 2, align: 1 }
        );
    }

    #[test]
    fn a_variant_with_a_u64_case_is_sixteen_bytes_and_joins_its_slots() {
        // Discriminant at 0, payload at 8.
        let space = [TypeDef::Variant(vec![
            Case {
                name: "a".into(),
                ty: Some(P(Primitive::U64)),
            },
            Case {
                name: "b".into(),
                ty: Some(P(Primitive::U8)),
            },
        ])];
        let ty = ValueType::Index(0);
        assert_eq!(layout(&space, &ty).unwrap(), Layout { size: 16, align: 8 });
        // The slot after the discriminant is i64, because i64 joins with i32 to i64.
        assert_eq!(flatten(&space, &ty).unwrap(), [ValType::I32, ValType::I64]);
    }

    #[test]
    fn a_variant_joins_f32_and_i32_to_i32() {
        let space = [TypeDef::Result {
            ok: Some(P(Primitive::F32)),
            err: Some(P(Primitive::U32)),
        }];
        assert_eq!(
            flatten(&space, &ValueType::Index(0)).unwrap(),
            [ValType::I32, ValType::I32]
        );
    }

    #[test]
    fn an_enum_takes_one_byte_up_to_256_cases_and_two_beyond() {
        let small = [TypeDef::Enum(vec![String::new(); 256])];
        assert_eq!(
            layout(&small, &ValueType::Index(0)).unwrap(),
            Layout { size: 1, align: 1 }
        );
        let large = [TypeDef::Enum(vec![String::new(); 257])];
        assert_eq!(
            layout(&large, &ValueType::Index(0)).unwrap(),
            Layout { size: 2, align: 2 }
        );
        assert_eq!(
            flatten(&large, &ValueType::Index(0)).unwrap(),
            [ValType::I32]
        );
    }

    #[test]
    fn flags_take_one_integer_per_32_labels() {
        let flags = |n: usize| [TypeDef::Flags(vec![String::new(); n])];
        let l = |n: usize| layout(&flags(n), &ValueType::Index(0)).unwrap();
        assert_eq!(l(3), Layout { size: 1, align: 1 });
        assert_eq!(l(9), Layout { size: 2, align: 2 });
        assert_eq!(l(32), Layout { size: 4, align: 4 });
        assert_eq!(l(33), Layout { size: 8, align: 4 });
        assert_eq!(
            flatten(&flags(33), &ValueType::Index(0)).unwrap(),
            [ValType::I32, ValType::I32]
        );
    }

    #[test]
    fn an_own_or_borrow_handle_is_one_i32() {
        let space = [TypeDef::Own(0), TypeDef::Borrow(0)];
        for i in 0..2 {
            assert_eq!(
                layout(&space, &ValueType::Index(i)).unwrap(),
                Layout { size: 4, align: 4 }
            );
            assert_eq!(
                flatten(&space, &ValueType::Index(i)).unwrap(),
                [ValType::I32]
            );
        }
    }

    #[test]
    fn a_list_is_a_pointer_and_a_length() {
        let space = [TypeDef::List(P(Primitive::U8))];
        assert_eq!(
            layout(&space, &ValueType::Index(0)).unwrap(),
            Layout { size: 8, align: 4 }
        );
    }

    #[test]
    fn a_function_type_is_not_a_value_type() {
        let space = [TypeDef::Func(FuncType {
            params: vec![],
            results: vec![],
        })];
        assert_eq!(
            layout(&space, &ValueType::Index(0)),
            Err(AbiError::NotAValueType(0))
        );
    }

    #[test]
    fn an_index_outside_the_space_is_refused() {
        assert_eq!(
            layout(&[], &ValueType::Index(3)),
            Err(AbiError::BadIndex(3))
        );
        assert_eq!(
            flatten(&[], &ValueType::Index(3)),
            Err(AbiError::BadIndex(3))
        );
    }

    #[test]
    fn an_empty_variant_is_refused() {
        let space = [TypeDef::Variant(vec![])];
        assert_eq!(
            layout(&space, &ValueType::Index(0)),
            Err(AbiError::EmptyVariant)
        );
    }

    #[test]
    fn a_self_referencing_type_is_refused_not_overflowed() {
        // A record whose only field is the record itself.
        let space = [TypeDef::Record(vec![f("me", ValueType::Index(0))])];
        assert_eq!(
            layout(&space, &ValueType::Index(0)),
            Err(AbiError::TooComplex)
        );
        assert_eq!(
            flatten(&space, &ValueType::Index(0)),
            Err(AbiError::TooComplex)
        );
    }

    #[test]
    fn a_long_chain_of_records_is_refused_at_the_depth_limit() {
        // Each record holds the one before it, so the chain is MAX_DEPTH + 2 long.
        let mut space = vec![TypeDef::Record(vec![])];
        for i in 0..(MAX_DEPTH as u32 + 2) {
            space.push(TypeDef::Record(vec![f("x", ValueType::Index(i))]));
        }
        let top = ValueType::Index(space.len() as u32 - 1);
        assert_eq!(layout(&space, &top), Err(AbiError::TooComplex));
    }

    #[test]
    fn doubling_references_hit_the_work_budget() {
        // Each record holds the previous one twice, so the size doubles at each level.
        let mut space = vec![TypeDef::Record(vec![])];
        for i in 0..40u32 {
            space.push(TypeDef::Record(vec![
                f("a", ValueType::Index(i)),
                f("b", ValueType::Index(i)),
            ]));
        }
        let top = ValueType::Index(space.len() as u32 - 1);
        assert_eq!(layout(&space, &top), Err(AbiError::TooComplex));
    }

    #[test]
    fn decodes_the_primitive_and_composite_forms() {
        // A type section: record {a: u8, b: u32}, variant {x, y(u32)}, list<string>,
        // option<u32>, and a function (u32) -> u32.
        let body: &[u8] = &[
            0x05, // five items
            0x72, 0x02, 0x01, b'a', 0x7d, 0x01, b'b', 0x79, //
            0x71, 0x02, 0x01, b'x', 0x00, 0x00, 0x01, b'y', 0x01, 0x79, 0x00, //
            0x70, 0x73, //
            0x6b, 0x79, //
            0x40, 0x01, 0x01, b'p', 0x79, 0x00, 0x79,
        ];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\0asm\x0d\0\x01\0");
        bytes.push(7);
        bytes.push(body.len() as u8);
        bytes.extend_from_slice(body);
        let c = parse(&bytes).unwrap();
        let types = c.sections[0].types.as_ref().unwrap();
        assert_eq!(types.len(), 5);
        assert_eq!(
            types[0],
            TypeDef::Record(vec![f("a", P(Primitive::U8)), f("b", P(Primitive::U32))])
        );
        assert_eq!(
            types[1],
            TypeDef::Variant(vec![
                Case {
                    name: "x".into(),
                    ty: None
                },
                Case {
                    name: "y".into(),
                    ty: Some(P(Primitive::U32))
                },
            ])
        );
        assert_eq!(types[2], TypeDef::List(P(Primitive::String)));
        assert_eq!(types[3], TypeDef::Option(P(Primitive::U32)));
        assert_eq!(
            types[4],
            TypeDef::Func(FuncType {
                params: vec![f("p", P(Primitive::U32))],
                results: vec![f("", P(Primitive::U32))],
            })
        );
    }

    #[test]
    fn a_type_index_above_the_one_byte_range_decodes() {
        // 116 bools, then list<index 115>: 115 needs two bytes as a signed
        // 33-bit integer, and it names the bool at position 115, which is defined.
        let mut body = vec![117u8];
        body.extend(core::iter::repeat_n(0x7Fu8, 116));
        body.extend_from_slice(&[0x70, 0xF3, 0x00]);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\0asm\x0d\0\x01\0");
        bytes.push(7);
        bytes.push(body.len() as u8);
        bytes.extend_from_slice(&body);
        let c = parse(&bytes).unwrap();
        assert_eq!(
            c.sections[0].types.as_ref().unwrap()[116],
            TypeDef::List(ValueType::Index(115))
        );
    }

    #[test]
    fn an_unsupported_type_kind_leaves_the_section_undecoded() {
        // 0x50 is a core module type, which this parser does not decode.
        let body: &[u8] = &[0x01, 0x50, 0x00];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\0asm\x0d\0\x01\0");
        bytes.push(7);
        bytes.push(body.len() as u8);
        bytes.extend_from_slice(body);
        let c = parse(&bytes).unwrap();
        assert_eq!(c.sections[0].types, None);
        assert_eq!(c.sections[0].count, Some(1));
    }

    #[test]
    fn a_bad_result_tag_is_a_malformed_type() {
        let body: &[u8] = &[0x01, 0x40, 0x00, 0x02, 0x79];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\0asm\x0d\0\x01\0");
        bytes.push(7);
        bytes.push(body.len() as u8);
        bytes.extend_from_slice(body);
        assert_eq!(
            parse(&bytes).unwrap_err().kind,
            ErrorKind::BadTypeEncoding(0x02)
        );
    }
}

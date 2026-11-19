//! Structural comparison of component types, for the linker.
//!
//! A type reference is an index into an index space. To compare two types, the
//! references are resolved to the types they name, and the resolved trees are
//! compared. Labels are compared for records and not for function parameters
//! and results, as the component model compares function types.
//!
//! A reference this module cannot follow is [`Ty::Unknown`]. That covers types
//! imported or aliased into a space, outer aliases, resources, component types
//! and undecoded types. A comparison that reaches an unknown gives no answer
//! (`None`), and the linker does not refuse a pair it cannot decide. Resource
//! identity is not compared across components, so resources are unknown too.
//!
//! Resolution is bounded in depth and work, so a type that refers to itself or
//! doubles at each level cannot exhaust the stack or the time.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::component::{Component, ExternDesc, ExternKind, Section, Space, TypeBound};
use crate::types::{Decl, Primitive, TypeDef, ValueType};

/// One entry of a type index space.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Entry<'a> {
    /// A type this crate decodes.
    Def(&'a TypeDef),
    /// A type this crate does not follow, such as an import or an alias.
    Unknown,
}

/// The deepest chain of references that resolution follows.
const MAX_DEPTH: usize = 64;
/// The most nodes one resolution may build.
const BUDGET: usize = 100_000;

/// The type index space of a component: one entry for each type it defines, in
/// section order. Type sections, type imports and type aliases contribute.
pub(crate) fn top_entries(sections: &[Section]) -> Vec<Entry<'_>> {
    let mut out = Vec::new();
    for s in sections {
        match s.id {
            7 => match &s.types {
                Some(defs) => out.extend(defs.iter().map(Entry::Def)),
                None => {
                    let n = s.count.unwrap_or(0) as usize;
                    out.extend(core::iter::repeat_n(Entry::Unknown, n));
                }
            },
            10 => {
                for import in s.imports.iter().flatten() {
                    if import.desc.kind() == ExternKind::Type {
                        out.push(Entry::Unknown);
                    }
                }
            }
            6 => {
                for alias in s.aliases.iter().flatten() {
                    if alias.space() == Some(Space::Type) {
                        out.push(Entry::Unknown);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The type index space that a list of declarations defines. A type export
/// names an existing type, and defines an entry that is that same type.
fn decl_entries(decls: &[Decl]) -> Vec<Entry<'_>> {
    let mut out = Vec::new();
    for decl in decls {
        match decl {
            Decl::Type(def) => out.push(Entry::Def(def)),
            Decl::Alias(alias) => {
                if alias.space() == Some(Space::Type) {
                    out.push(Entry::Unknown);
                }
            }
            Decl::Import(import) => {
                if import.desc.kind() == ExternKind::Type {
                    out.push(Entry::Unknown);
                }
            }
            Decl::Export {
                desc: ExternDesc::Type(TypeBound::Eq(i)),
                ..
            } => {
                let same = out.get(*i as usize).copied().unwrap_or(Entry::Unknown);
                out.push(same);
            }
            Decl::Export {
                desc: ExternDesc::Type(TypeBound::SubResource),
                ..
            } => out.push(Entry::Unknown),
            Decl::CoreType(_) | Decl::Export { .. } => {}
        }
    }
    out
}

/// A resolved type. Each reference has been replaced by the type it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    /// A type that cannot be followed.
    Unknown,
    /// A primitive value type.
    Prim(Primitive),
    /// A record, with its labels.
    Record(Vec<(String, Ty)>),
    /// A variant, with its case names and payloads.
    Variant(Vec<(String, Option<Ty>)>),
    /// A list.
    List(Box<Ty>),
    /// A tuple.
    Tuple(Vec<Ty>),
    /// Flag names.
    Flags(Vec<String>),
    /// Enum case names.
    Enum(Vec<String>),
    /// An option.
    Option(Box<Ty>),
    /// A result, with its optional payloads.
    Result(Option<Box<Ty>>, Option<Box<Ty>>),
    /// A function type. Parameter and result labels are not kept.
    Func {
        /// The parameter types, in order.
        params: Vec<Ty>,
        /// The result types, in order.
        results: Vec<Ty>,
    },
    /// An instance type: its exports.
    Instance(Vec<Export>),
}

/// An export of an instance type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Export {
    name: String,
    kind: ExternKind,
    ty: Ty,
}

/// Resolves type references, within a budget.
struct Resolver {
    budget: usize,
}

impl Resolver {
    /// Spends one unit of work, or reports that the work is too deep or too large.
    fn spend(&mut self, depth: usize) -> bool {
        if depth > MAX_DEPTH || self.budget == 0 {
            return false;
        }
        self.budget -= 1;
        true
    }

    /// The type at index `i` of `scope`. Its own references are resolved in the
    /// entries before it, so only those are passed down.
    fn index(&mut self, scope: &[Entry<'_>], i: u32, depth: usize) -> Ty {
        if !self.spend(depth) {
            return Ty::Unknown;
        }
        let i = i as usize;
        match scope.get(i) {
            Some(Entry::Def(def)) => self.def(def, &scope[..i], depth + 1),
            _ => Ty::Unknown,
        }
    }

    fn value(&mut self, scope: &[Entry<'_>], ty: &ValueType, depth: usize) -> Ty {
        match ty {
            ValueType::Primitive(p) => Ty::Prim(*p),
            ValueType::Index(i) => self.index(scope, *i, depth),
        }
    }

    fn opt(&mut self, scope: &[Entry<'_>], ty: &Option<ValueType>, depth: usize) -> Option<Ty> {
        ty.as_ref().map(|t| self.value(scope, t, depth))
    }

    fn def(&mut self, def: &TypeDef, scope: &[Entry<'_>], depth: usize) -> Ty {
        if !self.spend(depth) {
            return Ty::Unknown;
        }
        match def {
            TypeDef::Primitive(p) => Ty::Prim(*p),
            TypeDef::Record(fields) => Ty::Record(
                fields
                    .iter()
                    .map(|f| (f.name.clone(), self.value(scope, &f.ty, depth)))
                    .collect(),
            ),
            TypeDef::Variant(cases) => Ty::Variant(
                cases
                    .iter()
                    .map(|c| (c.name.clone(), self.opt(scope, &c.ty, depth)))
                    .collect(),
            ),
            TypeDef::List(t) => Ty::List(Box::new(self.value(scope, t, depth))),
            TypeDef::Tuple(ts) => {
                Ty::Tuple(ts.iter().map(|t| self.value(scope, t, depth)).collect())
            }
            TypeDef::Flags(names) => Ty::Flags(names.clone()),
            TypeDef::Enum(names) => Ty::Enum(names.clone()),
            TypeDef::Option(t) => Ty::Option(Box::new(self.value(scope, t, depth))),
            TypeDef::Result { ok, err } => {
                let ok = self.opt(scope, ok, depth).map(Box::new);
                let err = self.opt(scope, err, depth).map(Box::new);
                Ty::Result(ok, err)
            }
            TypeDef::Func(ft) => Ty::Func {
                params: ft
                    .params
                    .iter()
                    .map(|f| self.value(scope, &f.ty, depth))
                    .collect(),
                results: ft
                    .results
                    .iter()
                    .map(|f| self.value(scope, &f.ty, depth))
                    .collect(),
            },
            TypeDef::Instance(decls) => self.instance(decls, depth),
            // Resources and component types are not followed.
            TypeDef::Own(_)
            | TypeDef::Borrow(_)
            | TypeDef::Resource { .. }
            | TypeDef::Component(_) => Ty::Unknown,
        }
    }

    /// The exports of an instance type. Each export's reference is resolved in
    /// the type's own declarations.
    fn instance(&mut self, decls: &[Decl], depth: usize) -> Ty {
        let local = decl_entries(decls);
        let mut exports = Vec::new();
        for decl in decls {
            let Decl::Export { name, desc } = decl else {
                continue;
            };
            let ty = match desc {
                ExternDesc::Func(i)
                | ExternDesc::Instance(i)
                | ExternDesc::Type(TypeBound::Eq(i)) => self.index(&local, *i, depth),
                ExternDesc::Value(v) => self.value(&local, v, depth),
                ExternDesc::Type(TypeBound::SubResource)
                | ExternDesc::Component(_)
                | ExternDesc::CoreModule(_) => Ty::Unknown,
            };
            exports.push(Export {
                name: name.clone(),
                kind: desc.kind(),
                ty,
            });
        }
        Ty::Instance(exports)
    }
}

/// Combines answers: a definite `false` wins, then an unknown, then `true`.
fn all(answers: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    let mut unknown = false;
    for answer in answers {
        match answer {
            Some(false) => return Some(false),
            None => unknown = true,
            Some(true) => {}
        }
    }
    if unknown { None } else { Some(true) }
}

/// Whether two sequences of types are the same, element by element.
fn same_seq(a: &[Ty], b: &[Ty]) -> Option<bool> {
    if a.len() != b.len() {
        return Some(false);
    }
    all(a.iter().zip(b).map(|(x, y)| same(x, y)))
}

/// Whether a provider's instance type has every export the importer's instance
/// type asks for, with the same kind and a matching type. Extra exports are
/// allowed.
fn instance_fits(want: &[Export], have: &[Export]) -> Option<bool> {
    all(want
        .iter()
        .map(|w| match have.iter().find(|h| h.name == w.name) {
            None => Some(false),
            Some(h) if h.kind != w.kind => Some(false),
            Some(h) => same(&w.ty, &h.ty),
        }))
}

/// Whether two resolved types are the same. `None` means a part of either type
/// could not be followed, so the answer is not known.
pub(crate) fn same(a: &Ty, b: &Ty) -> Option<bool> {
    match (a, b) {
        (Ty::Unknown, _) | (_, Ty::Unknown) => None,
        (Ty::Prim(x), Ty::Prim(y)) => Some(x == y),
        (Ty::Record(x), Ty::Record(y)) => {
            if x.len() != y.len() {
                return Some(false);
            }
            all(x.iter().zip(y).map(
                |((n, t), (m, u))| {
                    if n == m { same(t, u) } else { Some(false) }
                },
            ))
        }
        (Ty::Variant(x), Ty::Variant(y)) => {
            if x.len() != y.len() {
                return Some(false);
            }
            all(x.iter().zip(y).map(|((n, t), (m, u))| {
                if n != m {
                    return Some(false);
                }
                match (t, u) {
                    (None, None) => Some(true),
                    (Some(t), Some(u)) => same(t, u),
                    _ => Some(false),
                }
            }))
        }
        (Ty::List(x), Ty::List(y)) | (Ty::Option(x), Ty::Option(y)) => same(x, y),
        (Ty::Tuple(x), Ty::Tuple(y)) => same_seq(x, y),
        (Ty::Flags(x), Ty::Flags(y)) | (Ty::Enum(x), Ty::Enum(y)) => Some(x == y),
        (Ty::Result(xo, xe), Ty::Result(yo, ye)) => {
            let opt = |p: &Option<Box<Ty>>, q: &Option<Box<Ty>>| match (p, q) {
                (None, None) => Some(true),
                (Some(p), Some(q)) => same(p, q),
                _ => Some(false),
            };
            all([opt(xo, yo), opt(xe, ye)].into_iter())
        }
        (
            Ty::Func {
                params: xp,
                results: xr,
            },
            Ty::Func {
                params: yp,
                results: yr,
            },
        ) => all([same_seq(xp, yp), same_seq(xr, yr)].into_iter()),
        (Ty::Instance(x), Ty::Instance(y)) => instance_fits(x, y),
        _ => Some(false),
    }
}

/// Whether a provider's declared export type fits an importer's import type.
///
/// `want` is the importer's descriptor, in `importer`, and `have` is the
/// provider's export descriptor, in `provider`. A pair with no declared type on
/// the provider side, or that cannot be followed, fits.
pub(crate) fn desc_fits(
    want: &ExternDesc,
    importer: &Component,
    have: Option<&ExternDesc>,
    provider: &Component,
) -> bool {
    let Some(have) = have else {
        return true;
    };
    let want_entries = top_entries(&importer.sections);
    let have_entries = top_entries(&provider.sections);
    let mut resolver = Resolver { budget: BUDGET };
    let (Some(w), Some(h)) = (
        resolve_desc(&mut resolver, &want_entries, want),
        resolve_desc(&mut resolver, &have_entries, have),
    ) else {
        return true;
    };
    same(&w, &h) != Some(false)
}

/// The type a descriptor declares, if it declares one that this module follows.
fn resolve_desc(resolver: &mut Resolver, scope: &[Entry<'_>], desc: &ExternDesc) -> Option<Ty> {
    let ty = match desc {
        ExternDesc::Func(i) | ExternDesc::Instance(i) | ExternDesc::Type(TypeBound::Eq(i)) => {
            resolver.index(scope, *i, 0)
        }
        ExternDesc::Value(v) => resolver.value(scope, v, 0),
        ExternDesc::Type(TypeBound::SubResource)
        | ExternDesc::Component(_)
        | ExternDesc::CoreModule(_) => return None,
    };
    match ty {
        Ty::Unknown => None,
        ty => Some(ty),
    }
}

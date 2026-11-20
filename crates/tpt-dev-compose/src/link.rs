//! Static linking of core modules: resolving imports to exports, checking
//! their types, and ordering instantiation.
//!
//! The linker plans. It does not instantiate or run anything. [`Registry::link`]
//! returns a [`Plan`] that gives the instantiation order and the export each
//! import is bound to. [`crate::runtime::Runtime::instantiate`] follows a plan
//! to instantiate the core modules, and runs the integer subset described in
//! [`crate::runtime`].
//!
//! Component-level imports are resolved too, by name and kind, against the
//! exports of the registered components. A component import may be bound only
//! to a single export of its kind, with that name, across all registered
//! components. The types of function, value, type and instance imports are
//! compared structurally where both sides can be followed (see
//! `crate::compare`). An instance may export more than the importer asks for.
//! A pair that cannot be followed, such as one involving a resource or an alias,
//! is checked by name and kind only. The plan records these
//! bindings as data. Nothing here instantiates a component either.
//!
//! Every core import kind is resolved: functions, tables, memories, globals and
//! tags. Each one is checked against its export at the level of types. A
//! function's signature must match exactly. A table's element type must match,
//! and its limits must fit within the imported ones. A memory's limits must
//! fit. A global's value type and mutability must match. A tag's signature
//! must match.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::compare::desc_fits;
use crate::component::{Component, ExternKind};
use crate::module::{ExportKind, ImportDesc, Limits, Module};

/// Why a set of modules does not link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// Two modules were registered under the same name.
    DuplicateName(String),
    /// An import names a module that was not registered.
    UnknownModule {
        /// The importing module.
        importer: String,
        /// The module the import names.
        module: String,
    },
    /// The provider has no export of the imported kind with the imported name.
    UnknownExport {
        /// The importing module.
        importer: String,
        /// The module the import names.
        module: String,
        /// The name the import asks for.
        name: String,
        /// The kind of export the import needs.
        kind: ExportKind,
    },
    /// The export's type does not match the import's.
    TypeMismatch {
        /// The importing module.
        importer: String,
        /// The module the import names.
        module: String,
        /// The name the import asks for.
        name: String,
        /// The kind of export the import needs.
        kind: ExportKind,
    },
    /// The modules depend on each other in a cycle. These modules could not be ordered.
    Cycle(Vec<String>),
    /// A component import has no export of its kind and name in any registered component.
    UnknownComponentExport {
        /// The importing component.
        importer: String,
        /// The name the import asks for.
        name: String,
        /// The kind of export the import needs.
        kind: ExternKind,
    },
    /// More than one registered component exports the imported name with the imported kind.
    AmbiguousComponentExport {
        /// The importing component.
        importer: String,
        /// The name the import asks for.
        name: String,
        /// The kind of export the import needs.
        kind: ExternKind,
    },
    /// The provider's declared type does not match the import's type.
    ComponentTypeMismatch {
        /// The importing component.
        importer: String,
        /// The name the import asks for.
        name: String,
        /// The kind of export the import needs.
        kind: ExternKind,
    },
}

fn kind_name(kind: ExportKind) -> &'static str {
    match kind {
        ExportKind::Func => "function",
        ExportKind::Table => "table",
        ExportKind::Memory => "memory",
        ExportKind::Global => "global",
        ExportKind::Tag => "tag",
    }
}

fn extern_kind_name(kind: ExternKind) -> &'static str {
    match kind {
        ExternKind::CoreModule => "core module",
        ExternKind::Func => "function",
        ExternKind::Value => "value",
        ExternKind::Type => "type",
        ExternKind::Instance => "instance",
        ExternKind::Component => "component",
    }
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::DuplicateName(n) => write!(f, "module {n} is registered twice"),
            LinkError::UnknownModule { importer, module } => {
                write!(f, "{importer} imports from unknown module {module}")
            }
            LinkError::UnknownExport {
                importer,
                module,
                name,
                kind,
            } => write!(
                f,
                "{importer} imports {module}.{name}, which is not exported as a {}",
                kind_name(*kind)
            ),
            LinkError::TypeMismatch {
                importer,
                module,
                name,
                kind,
            } => write!(
                f,
                "{importer} imports {module}.{name} with the wrong {} type",
                kind_name(*kind)
            ),
            LinkError::Cycle(names) => {
                write!(f, "modules depend on each other: {}", names.join(", "))
            }
            LinkError::UnknownComponentExport {
                importer,
                name,
                kind,
            } => write!(
                f,
                "{importer} imports {name}, which no registered component exports as a {}",
                extern_kind_name(*kind)
            ),
            LinkError::AmbiguousComponentExport {
                importer,
                name,
                kind,
            } => write!(
                f,
                "{importer} imports {name}, which more than one component exports as a {}",
                extern_kind_name(*kind)
            ),
            LinkError::ComponentTypeMismatch {
                importer,
                name,
                kind,
            } => write!(
                f,
                "{importer} imports {name} with the wrong {} type",
                extern_kind_name(*kind)
            ),
        }
    }
}

/// One resolved import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    /// Index of the importing module in the registry.
    pub importer: usize,
    /// Index of the import in the importer's import list.
    pub import: usize,
    /// Index of the providing module in the registry.
    pub provider: usize,
    /// Index of the export in the provider's export list.
    pub export: usize,
    /// The kind of the import, which is also the kind of the export.
    pub kind: ExportKind,
}

/// One resolved component-level import.
///
/// The indices are positions in the flattened import and export lists of the
/// components, as [`Component::imports`] and [`Component::exports`] give them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentBinding {
    /// Index of the importing component in the registry's component list.
    pub importer: usize,
    /// Index of the import in the importer's flattened import list.
    pub import: usize,
    /// Index of the providing component in the registry's component list.
    pub provider: usize,
    /// Index of the export in the provider's flattened export list.
    pub export: usize,
    /// The kind of the import, which is also the kind of the export.
    pub kind: ExternKind,
}

/// A checked plan for instantiating a set of modules and components.
///
/// The plan is data only. Nothing here instantiates a module or a component,
/// or runs code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Registered module names, in registration order.
    pub names: Vec<String>,
    /// Module registry indices in an order where every provider comes before its consumers.
    pub order: Vec<usize>,
    /// Every resolved core-module import, of every kind.
    pub bindings: Vec<Binding>,
    /// Registered component names, in registration order.
    pub component_names: Vec<String>,
    /// Component registry indices in an order where every provider comes before its consumers.
    pub component_order: Vec<usize>,
    /// Every resolved component-level import, of every kind.
    pub component_bindings: Vec<ComponentBinding>,
}

/// A set of named modules and components to link together.
///
/// Module and component names share one namespace, so a name is registered once.
#[derive(Debug, Default)]
pub struct Registry<'a> {
    pub(crate) modules: Vec<(String, &'a Module)>,
    components: Vec<(String, &'a Component)>,
}

/// Whether a provider's limits satisfy an import's limits: the provider's
/// minimum is at least the import's, and a bounded import needs a bounded
/// provider no larger than it.
fn limits_fit(provided: Limits, wanted: Limits) -> bool {
    if provided.min < wanted.min {
        return false;
    }
    match wanted.max {
        None => true,
        Some(wm) => matches!(provided.max, Some(pm) if pm <= wm),
    }
}

impl<'a> Registry<'a> {
    /// An empty registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::Registry;
    ///
    /// let registry = Registry::new();
    /// // An empty registry links trivially.
    /// assert!(registry.link().is_ok());
    /// ```
    pub const fn new() -> Self {
        Self {
            modules: Vec::new(),
            components: Vec::new(),
        }
    }

    /// Registers `module` under `name`, and returns its index.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::{Registry, parse};
    ///
    /// const BYTES: &[u8] = &[
    ///     0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
    ///     0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // type: () -> ()
    ///     0x03, 0x02, 0x01, 0x00, // function: one, of type 0
    ///     0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, // export "run" = func 0
    ///     0x0a, 0x04, 0x01, 0x02, 0x00, 0x0b, // code: one empty body
    /// ];
    ///
    /// let module = parse(BYTES).unwrap();
    /// let mut registry = Registry::new();
    /// assert_eq!(registry.register("math", &module).unwrap(), 0);
    /// // A second module under the same name is refused.
    /// assert!(registry.register("math", &module).is_err());
    /// ```
    pub fn register(&mut self, name: &str, module: &'a Module) -> Result<usize, LinkError> {
        if self.taken(name) {
            return Err(LinkError::DuplicateName(String::from(name)));
        }
        self.modules.push((String::from(name), module));
        Ok(self.modules.len() - 1)
    }

    /// Registers `component` under `name`, and returns its index in the component list.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::{Registry, parse_component};
    ///
    /// const ADD: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/component/add.wasm"));
    ///
    /// let component = parse_component(ADD).unwrap();
    /// let mut registry = Registry::new();
    /// assert_eq!(registry.register_component("adder", &component).unwrap(), 0);
    /// ```
    pub fn register_component(
        &mut self,
        name: &str,
        component: &'a Component,
    ) -> Result<usize, LinkError> {
        if self.taken(name) {
            return Err(LinkError::DuplicateName(String::from(name)));
        }
        self.components.push((String::from(name), component));
        Ok(self.components.len() - 1)
    }

    fn taken(&self, name: &str) -> bool {
        self.find(name).is_some() || self.components.iter().any(|(n, _)| n == name)
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.modules.iter().position(|(n, _)| n == name)
    }

    /// Resolves every import, checks every type, and orders the modules and components.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::{Registry, parse};
    ///
    /// const BYTES: &[u8] = &[
    ///     0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
    ///     0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // type: () -> ()
    ///     0x03, 0x02, 0x01, 0x00, // function: one, of type 0
    ///     0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, // export "run" = func 0
    ///     0x0a, 0x04, 0x01, 0x02, 0x00, 0x0b, // code: one empty body
    /// ];
    ///
    /// let module = parse(BYTES).unwrap();
    /// let mut registry = Registry::new();
    /// registry.register("math", &module).unwrap();
    /// // The module has no imports, so every import resolves.
    /// assert!(registry.link().is_ok());
    /// ```
    pub fn link(&self) -> Result<Plan, LinkError> {
        let (bindings, order) = self.link_modules()?;
        let (component_bindings, component_order) = self.link_components()?;
        Ok(Plan {
            names: self.modules.iter().map(|(n, _)| n.clone()).collect(),
            order,
            bindings,
            component_names: self.components.iter().map(|(n, _)| n.clone()).collect(),
            component_order,
            component_bindings,
        })
    }

    /// Resolves the core-module imports, and returns the bindings and the order.
    fn link_modules(&self) -> Result<(Vec<Binding>, Vec<usize>), LinkError> {
        let n = self.modules.len();
        let mut bindings = Vec::new();
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (i, (importer, module)) in self.modules.iter().enumerate() {
            for (k, import) in module.imports.iter().enumerate() {
                let kind = import.desc.kind();
                let Some(j) = self.find(&import.module) else {
                    return Err(LinkError::UnknownModule {
                        importer: importer.clone(),
                        module: import.module.clone(),
                    });
                };
                let provider = self.modules[j].1;
                let Some(e) = provider
                    .exports
                    .iter()
                    .position(|x| x.kind == kind && x.name == import.name)
                else {
                    return Err(LinkError::UnknownExport {
                        importer: importer.clone(),
                        module: import.module.clone(),
                        name: import.name.clone(),
                        kind,
                    });
                };
                let index = provider.exports[e].index;
                let fits = match import.desc {
                    ImportDesc::Func(t) => {
                        module.types.get(t as usize).is_some()
                            && provider.func_type(index) == module.types.get(t as usize)
                    }
                    ImportDesc::Table(want) => provider.table_type(index).is_some_and(|have| {
                        have.elem == want.elem && limits_fit(have.limits, want.limits)
                    }),
                    ImportDesc::Memory(want) => provider
                        .memory_type(index)
                        .is_some_and(|have| limits_fit(have, want)),
                    ImportDesc::Global(want) => provider.global_type(index) == Some(want),
                    ImportDesc::Tag(t) => {
                        match (provider.tag_type(index), module.types.get(t as usize)) {
                            (Some(p), Some(wanted)) => {
                                provider.types.get(p as usize) == Some(wanted)
                            }
                            _ => false,
                        }
                    }
                };
                if !fits {
                    return Err(LinkError::TypeMismatch {
                        importer: importer.clone(),
                        module: import.module.clone(),
                        name: import.name.clone(),
                        kind,
                    });
                }
                bindings.push(Binding {
                    importer: i,
                    import: k,
                    provider: j,
                    export: e,
                    kind,
                });
                if !deps[i].contains(&j) {
                    deps[i].push(j);
                }
            }
        }

        let order = topological_order(&deps).map_err(|stuck| {
            LinkError::Cycle(stuck.iter().map(|&i| self.modules[i].0.clone()).collect())
        })?;
        Ok((bindings, order))
    }

    /// Resolves the component-level imports, and returns the bindings and the order.
    fn link_components(&self) -> Result<(Vec<ComponentBinding>, Vec<usize>), LinkError> {
        let n = self.components.len();
        let mut bindings = Vec::new();
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (i, (importer, component)) in self.components.iter().enumerate() {
            for (k, import) in component.imports().enumerate() {
                let kind = import.desc.kind();
                let mut matches = 0usize;
                let mut found = None;
                for (j, (_, provider)) in self.components.iter().enumerate() {
                    for (e, export) in provider.exports().enumerate() {
                        if export.kind == kind && export.name == import.name {
                            matches += 1;
                            found = Some((j, e, *provider, export));
                        }
                    }
                }
                let (j, e, provider, export) = match found {
                    Some(f) if matches == 1 => f,
                    _ if matches == 0 => {
                        return Err(LinkError::UnknownComponentExport {
                            importer: importer.clone(),
                            name: import.name.clone(),
                            kind,
                        });
                    }
                    _ => {
                        return Err(LinkError::AmbiguousComponentExport {
                            importer: importer.clone(),
                            name: import.name.clone(),
                            kind,
                        });
                    }
                };
                if !desc_fits(&import.desc, component, export.desc.as_ref(), provider) {
                    return Err(LinkError::ComponentTypeMismatch {
                        importer: importer.clone(),
                        name: import.name.clone(),
                        kind,
                    });
                }
                bindings.push(ComponentBinding {
                    importer: i,
                    import: k,
                    provider: j,
                    export: e,
                    kind,
                });
                if !deps[i].contains(&j) {
                    deps[i].push(j);
                }
            }
        }

        let order = topological_order(&deps).map_err(|stuck| {
            LinkError::Cycle(
                stuck
                    .iter()
                    .map(|&i| self.components[i].0.clone())
                    .collect(),
            )
        })?;
        Ok((bindings, order))
    }
}

/// Orders nodes so that every node comes after the nodes it depends on. On a
/// cycle, returns the nodes that could not be placed.
fn topological_order(deps: &[Vec<usize>]) -> Result<Vec<usize>, Vec<usize>> {
    let n = deps.len();
    // Kahn's algorithm: a node is ready once everything it depends on is placed.
    let mut waiting: Vec<usize> = deps.iter().map(Vec::len).collect();
    let mut placed = vec![false; n];
    let mut order = Vec::with_capacity(n);
    while let Some(next) = (0..n).find(|&i| !placed[i] && waiting[i] == 0) {
        placed[next] = true;
        order.push(next);
        for (i, d) in deps.iter().enumerate() {
            if d.contains(&next) {
                waiting[i] -= 1;
            }
        }
    }
    if order.len() == n {
        Ok(order)
    } else {
        Err((0..n).filter(|&i| !placed[i]).collect())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::module::{Export, FuncType, GlobalType, Import, TableType, ValType};

    #[test]
    fn a_larger_provider_minimum_fits_a_smaller_requirement() {
        let have = Limits {
            min: 4,
            max: Some(8),
        };
        let want = |min, max| Limits { min, max };
        assert!(limits_fit(have, want(2, Some(8))));
        assert!(limits_fit(have, want(2, None)));
        assert!(!limits_fit(have, want(5, None)));
        assert!(!limits_fit(have, want(2, Some(7))));
        assert!(!limits_fit(want(4, None), want(2, Some(8))));
    }

    /// A provider exporting a table "t", a memory "m" and a global "g", and an
    /// importer that imports `imports` from it.
    fn pair(imports: &[(&str, ExportKind, ImportDesc)], global: GlobalType) -> (Module, Module) {
        let provider = Module {
            types: vec![FuncType {
                params: vec![],
                results: vec![],
            }],
            tables: vec![TableType {
                elem: ValType::FuncRef,
                limits: Limits {
                    min: 2,
                    max: Some(4),
                },
            }],
            memories: vec![Limits { min: 1, max: None }],
            globals: vec![global],
            exports: vec![
                Export {
                    name: "t".into(),
                    kind: ExportKind::Table,
                    index: 0,
                },
                Export {
                    name: "m".into(),
                    kind: ExportKind::Memory,
                    index: 0,
                },
                Export {
                    name: "g".into(),
                    kind: ExportKind::Global,
                    index: 0,
                },
            ],
            ..Module::default()
        };
        let importer = Module {
            types: provider.types.clone(),
            imports: imports
                .iter()
                .map(|(name, _, desc)| Import {
                    module: "p".into(),
                    name: (*name).into(),
                    desc: *desc,
                })
                .collect(),
            ..Module::default()
        };
        (provider, importer)
    }

    fn i32_const() -> GlobalType {
        GlobalType {
            ty: ValType::I32,
            mutable: false,
        }
    }

    fn table(elem: ValType, min: u32, max: Option<u32>) -> ImportDesc {
        ImportDesc::Table(TableType {
            elem,
            limits: Limits { min, max },
        })
    }

    fn link_pair(provider: &Module, importer: &Module) -> Result<Plan, LinkError> {
        let mut r = Registry::new();
        r.register("p", provider).unwrap();
        r.register("i", importer).unwrap();
        r.link()
    }

    #[test]
    fn table_memory_and_global_imports_link_and_are_bound() {
        let imports = [
            ("t", ExportKind::Table, table(ValType::FuncRef, 1, Some(4))),
            (
                "m",
                ExportKind::Memory,
                ImportDesc::Memory(Limits { min: 1, max: None }),
            ),
            ("g", ExportKind::Global, ImportDesc::Global(i32_const())),
        ];
        let (p, i) = pair(&imports, i32_const());
        let plan = link_pair(&p, &i).unwrap();
        assert_eq!(plan.order, [0, 1]);
        let kinds: Vec<ExportKind> = plan.bindings.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [ExportKind::Table, ExportKind::Memory, ExportKind::Global]
        );
        assert!(
            plan.bindings
                .iter()
                .all(|b| b.provider == 0 && b.importer == 1)
        );
    }

    #[test]
    fn a_wrong_element_type_is_a_table_mismatch() {
        let imports = [("t", ExportKind::Table, table(ValType::ExternRef, 1, None))];
        let (p, i) = pair(&imports, i32_const());
        assert!(matches!(
            link_pair(&p, &i),
            Err(LinkError::TypeMismatch {
                kind: ExportKind::Table,
                ..
            })
        ));
    }

    #[test]
    fn a_table_whose_limits_do_not_fit_is_a_mismatch() {
        // The provider's maximum of 4 is above the importer's bound of 3.
        let imports = [("t", ExportKind::Table, table(ValType::FuncRef, 1, Some(3)))];
        let (p, i) = pair(&imports, i32_const());
        assert!(matches!(
            link_pair(&p, &i),
            Err(LinkError::TypeMismatch {
                kind: ExportKind::Table,
                ..
            })
        ));
    }

    #[test]
    fn a_mutable_global_does_not_satisfy_an_immutable_import() {
        let imports = [("g", ExportKind::Global, ImportDesc::Global(i32_const()))];
        let mutable = GlobalType {
            ty: ValType::I32,
            mutable: true,
        };
        let (p, i) = pair(&imports, mutable);
        assert!(matches!(
            link_pair(&p, &i),
            Err(LinkError::TypeMismatch {
                kind: ExportKind::Global,
                ..
            })
        ));
    }

    #[test]
    fn an_import_of_the_wrong_kind_is_not_exported() {
        // "t" is a table in the provider, so importing it as a memory finds nothing.
        let imports = [(
            "t",
            ExportKind::Memory,
            ImportDesc::Memory(Limits { min: 1, max: None }),
        )];
        let (p, i) = pair(&imports, i32_const());
        assert!(matches!(
            link_pair(&p, &i),
            Err(LinkError::UnknownExport {
                kind: ExportKind::Memory,
                ..
            })
        ));
    }

    #[test]
    fn a_tag_import_with_no_tag_export_is_refused() {
        let imports = [("e", ExportKind::Tag, ImportDesc::Tag(0))];
        let (p, i) = pair(&imports, i32_const());
        assert!(matches!(
            link_pair(&p, &i),
            Err(LinkError::UnknownExport {
                kind: ExportKind::Tag,
                ..
            })
        ));
    }
}

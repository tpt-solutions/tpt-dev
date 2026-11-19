//! Static linking of core modules: resolving imports to exports, checking
//! their types, and ordering instantiation.
//!
//! The linker plans. It does not instantiate or run anything. [`Registry::link`]
//! returns a [`Plan`] that gives the instantiation order and the export each
//! import is bound to. Executing a plan needs a WebAssembly engine, which this
//! crate does not provide.
//!
//! Every import kind is resolved: functions, tables, memories, globals and
//! tags. Each one is checked against its export at the level of types. A
//! function's signature must match exactly. A table's element type must match,
//! and its limits must fit within the imported ones. A memory's limits must
//! fit. A global's value type and mutability must match. A tag's signature
//! must match.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

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

/// A checked plan for instantiating a set of modules.
///
/// The plan is data only. Nothing here instantiates a module or runs code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Registered names, in registration order.
    pub names: Vec<String>,
    /// Registry indices in an order where every provider comes before its consumers.
    pub order: Vec<usize>,
    /// Every resolved import, of every kind.
    pub bindings: Vec<Binding>,
}

/// A set of named modules to link together.
#[derive(Debug, Default)]
pub struct Registry<'a> {
    modules: Vec<(String, &'a Module)>,
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
    pub const fn new() -> Self {
        Self {
            modules: Vec::new(),
        }
    }

    /// Registers `module` under `name`, and returns its index.
    pub fn register(&mut self, name: &str, module: &'a Module) -> Result<usize, LinkError> {
        if self.find(name).is_some() {
            return Err(LinkError::DuplicateName(String::from(name)));
        }
        self.modules.push((String::from(name), module));
        Ok(self.modules.len() - 1)
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.modules.iter().position(|(n, _)| n == name)
    }

    /// Resolves every import, checks every type, and orders the modules.
    pub fn link(&self) -> Result<Plan, LinkError> {
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

        // Kahn's algorithm: a module is ready once everything it imports from is placed.
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
        if order.len() != n {
            let stuck = (0..n)
                .filter(|&i| !placed[i])
                .map(|i| self.modules[i].0.clone())
                .collect();
            return Err(LinkError::Cycle(stuck));
        }

        Ok(Plan {
            names: self.modules.iter().map(|(n, _)| n.clone()).collect(),
            order,
            bindings,
        })
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

//! Static linking of core modules: resolving imports to exports, checking
//! signatures, and ordering instantiation.
//!
//! The linker plans. It does not instantiate or run anything. Only function
//! imports can be resolved here. Other kinds of import are refused, because
//! their types would need checking too.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::module::{ExportKind, ImportDesc, Module};

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
    /// The provider has no function export with the imported name.
    UnknownExport {
        /// The importing module.
        importer: String,
        /// The module the import names.
        module: String,
        /// The name the import asks for.
        name: String,
    },
    /// The import is not a function, which this linker does not resolve.
    UnsupportedImport {
        /// The importing module.
        importer: String,
        /// The module the import names.
        module: String,
        /// The name the import asks for.
        name: String,
    },
    /// The export's signature differs from the import's.
    TypeMismatch {
        /// The importing module.
        importer: String,
        /// The module the import names.
        module: String,
        /// The name the import asks for.
        name: String,
    },
    /// The modules depend on each other in a cycle. These modules could not be ordered.
    Cycle(Vec<String>),
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
            } => write!(
                f,
                "{importer} imports {module}.{name}, which is not exported"
            ),
            LinkError::UnsupportedImport {
                importer,
                module,
                name,
            } => write!(
                f,
                "{importer} imports {module}.{name}, which is not a function"
            ),
            LinkError::TypeMismatch {
                importer,
                module,
                name,
            } => write!(
                f,
                "{importer} imports {module}.{name} with the wrong signature"
            ),
            LinkError::Cycle(names) => {
                write!(f, "modules depend on each other: {}", names.join(", "))
            }
        }
    }
}

/// One resolved function import.
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
}

/// A checked plan for instantiating a set of modules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Registered names, in registration order.
    pub names: Vec<String>,
    /// Registry indices in an order where every provider comes before its consumers.
    pub order: Vec<usize>,
    /// Every resolved function import.
    pub bindings: Vec<Binding>,
}

/// A set of named modules to link together.
#[derive(Debug, Default)]
pub struct Registry<'a> {
    modules: Vec<(String, &'a Module)>,
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

    /// Resolves every import, checks every signature, and orders the modules.
    pub fn link(&self) -> Result<Plan, LinkError> {
        let n = self.modules.len();
        let mut bindings = Vec::new();
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (i, (importer, module)) in self.modules.iter().enumerate() {
            for (k, import) in module.imports.iter().enumerate() {
                let ImportDesc::Func(type_index) = import.desc else {
                    return Err(LinkError::UnsupportedImport {
                        importer: importer.clone(),
                        module: import.module.clone(),
                        name: import.name.clone(),
                    });
                };
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
                    .position(|x| x.kind == ExportKind::Func && x.name == import.name)
                else {
                    return Err(LinkError::UnknownExport {
                        importer: importer.clone(),
                        module: import.module.clone(),
                        name: import.name.clone(),
                    });
                };
                let wanted = module.types.get(type_index as usize);
                let have = provider.func_type(provider.exports[e].index);
                if have.is_none() || have != wanted {
                    return Err(LinkError::TypeMismatch {
                        importer: importer.clone(),
                        module: import.module.clone(),
                        name: import.name.clone(),
                    });
                }
                bindings.push(Binding {
                    importer: i,
                    import: k,
                    provider: j,
                    export: e,
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

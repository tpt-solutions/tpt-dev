//! The dependency graph shared by the `Cargo.lock` and `cargo metadata` readers.

/// Which kind of dependency an edge is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DepKind {
    /// A normal dependency.
    Normal,
    /// A dev-dependency (`[dev-dependencies]`).
    Dev,
    /// A build-dependency (`[build-dependencies]`).
    Build,
}

/// Which edge kinds the analysis follows.
///
/// Normal edges are always followed. Dev and build edges are followed only when
/// asked. `Cargo.lock` does not record kinds, so every lock-file edge is normal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    /// Follow dev-dependency edges.
    pub dev: bool,
    /// Follow build-dependency edges.
    pub build: bool,
}

impl Scope {
    /// Follows every kind of edge.
    pub const ALL: Scope = Scope {
        dev: true,
        build: true,
    };

    /// Returns `true` if edges of `kind` are followed.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::{DepKind, Scope};
    ///
    /// assert!(Scope::default().includes(DepKind::Normal));
    /// assert!(!Scope::default().includes(DepKind::Dev));
    /// assert!(Scope::ALL.includes(DepKind::Build));
    /// ```
    pub fn includes(self, kind: DepKind) -> bool {
        match kind {
            DepKind::Normal => true,
            DepKind::Dev => self.dev,
            DepKind::Build => self.build,
        }
    }
}

/// One outgoing edge of a package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    /// The target, as a name, or `name version` when several versions are present.
    /// [`Graph::resolve`] turns it into a package index.
    pub target: String,
    /// The kind of dependency.
    pub kind: DepKind,
    /// Whether the manifest marks the dependency as optional.
    pub optional: bool,
}

impl Dependency {
    /// A normal, non-optional edge to `target`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::Dependency;
    ///
    /// let dep = Dependency::normal("serde");
    /// assert_eq!(dep.target, "serde");
    /// assert!(!dep.optional);
    /// ```
    pub fn normal(target: impl Into<String>) -> Self {
        Dependency {
            target: target.into(),
            kind: DepKind::Normal,
            optional: false,
        }
    }
}

/// One package in the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    /// Crate name.
    pub name: String,
    /// Exact version.
    pub version: String,
    /// Where the package comes from, such as `registry+...`, or `None` for a
    /// path or workspace package.
    pub source: Option<String>,
    /// Outgoing edges.
    pub dependencies: Vec<Dependency>,
}

/// The dependency graph of a workspace.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Graph {
    /// Packages, in input order.
    pub packages: Vec<Package>,
}

impl Graph {
    /// Index of the package matching a dependency target.
    ///
    /// A target is either `name` (when only one version exists) or `name version`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::{Graph, Package};
    ///
    /// let graph = Graph {
    ///     packages: vec![Package {
    ///         name: "serde".into(),
    ///         version: "1.0.0".into(),
    ///         source: None,
    ///         dependencies: vec![],
    ///     }],
    /// };
    /// assert_eq!(graph.resolve("serde"), Some(0));
    /// assert_eq!(graph.resolve("serde 1.0.0"), Some(0));
    /// assert_eq!(graph.resolve("serde 2.0.0"), None);
    /// ```
    pub fn resolve(&self, target: &str) -> Option<usize> {
        let mut parts = target.split_whitespace();
        let name = parts.next()?;
        let version = parts.next();
        self.packages
            .iter()
            .position(|p| p.name == name && version.is_none_or(|v| p.version == v))
    }

    /// Each package's direct dependencies, as package indices, following only
    /// the edge kinds in `scope`. Unresolvable targets are dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::{Dependency, Graph, Package, Scope};
    ///
    /// let pkg = |name: &str, deps: Vec<Dependency>| Package {
    ///     name: name.into(),
    ///     version: "1.0.0".into(),
    ///     source: None,
    ///     dependencies: deps,
    /// };
    /// let graph = Graph {
    ///     packages: vec![pkg("app", vec![Dependency::normal("lib")]), pkg("lib", vec![])],
    /// };
    /// assert_eq!(graph.edges(Scope::default()), vec![vec![1], vec![]]);
    /// ```
    pub fn edges(&self, scope: Scope) -> Vec<Vec<usize>> {
        self.packages
            .iter()
            .map(|p| {
                p.dependencies
                    .iter()
                    .filter(|d| scope.includes(d.kind))
                    .filter_map(|d| self.resolve(&d.target))
                    .collect()
            })
            .collect()
    }
}

/// Parses the text of a `Cargo.lock` file.
///
/// Reads the `[[package]]` tables for `name`, `version`, `source` and the
/// `dependencies` array. Other fields are ignored, so lock-file versions 3 and 4
/// both parse. Every lock-file dependency is a normal, non-optional edge.
///
/// ```
/// use tpt_dev_dep::{Scope, parse_lock};
///
/// let g = parse_lock(
///     "[[package]]\nname = \"app\"\nversion = \"1.0.0\"\ndependencies = [\"lib\"]\n\n\
///      [[package]]\nname = \"lib\"\nversion = \"0.1.0\"\n",
/// ).unwrap();
/// assert_eq!(g.packages.len(), 2);
/// assert_eq!(g.edges(Scope::default())[0], vec![1]);
/// ```
pub fn parse_lock(text: &str) -> Result<Graph, String> {
    let mut packages: Vec<Package> = Vec::new();
    let mut current: Option<Package> = None;
    let mut in_deps = false;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if in_deps {
            if line.starts_with(']') {
                in_deps = false;
                continue;
            }
            let dep =
                quoted(line).ok_or_else(|| format!("line {}: bad dependency entry", i + 1))?;
            if let Some(p) = current.as_mut() {
                p.dependencies.push(Dependency::normal(dep));
            }
            continue;
        }
        if line == "[[package]]" {
            if let Some(p) = current.take() {
                packages.push(p);
            }
            current = Some(Package {
                name: String::new(),
                version: String::new(),
                source: None,
                dependencies: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("name = ") {
            if let Some(p) = current.as_mut() {
                p.name = quoted(rest).ok_or_else(|| format!("line {}: bad name", i + 1))?;
            }
        } else if let Some(rest) = line.strip_prefix("version = ") {
            if let Some(p) = current.as_mut() {
                p.version = quoted(rest).ok_or_else(|| format!("line {}: bad version", i + 1))?;
            }
        } else if let Some(rest) = line.strip_prefix("source = ") {
            if let Some(p) = current.as_mut() {
                p.source = Some(quoted(rest).ok_or_else(|| format!("line {}: bad source", i + 1))?);
            }
        } else if let Some(rest) = line.strip_prefix("dependencies = ") {
            let rest = rest.trim();
            if rest == "[" {
                in_deps = true;
            } else if let Some(inner) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                // A single-line array, such as ["a", "b 1.0"].
                if let Some(p) = current.as_mut() {
                    for item in inner.split(',').filter(|s| !s.trim().is_empty()) {
                        let dep = quoted(item)
                            .ok_or_else(|| format!("line {}: bad dependency entry", i + 1))?;
                        p.dependencies.push(Dependency::normal(dep));
                    }
                }
            } else {
                return Err(format!("line {}: unsupported dependencies syntax", i + 1));
            }
        }
    }
    if let Some(p) = current.take() {
        packages.push(p);
    }
    if packages
        .iter()
        .any(|p| p.name.is_empty() || p.version.is_empty())
    {
        return Err(String::from("a package is missing its name or version"));
    }
    Ok(Graph { packages })
}

/// The text inside the first pair of double quotes in `s`.
fn quoted(s: &str) -> Option<String> {
    let start = s.find('"')? + 1;
    let end = start + s.get(start..)?.find('"')?;
    Some(s.get(start..end)?.to_string())
}

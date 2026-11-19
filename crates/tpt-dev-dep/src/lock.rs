//! Parsing `Cargo.lock` and building the dependency graph.

/// One package in the lock file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    /// Crate name.
    pub name: String,
    /// Exact version.
    pub version: String,
    /// Dependencies, as written in the lock file: a name, or `name version` when
    /// several versions are present.
    pub dependencies: Vec<String>,
}

/// The dependency graph of a workspace.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Graph {
    /// Packages, in lock-file order.
    pub packages: Vec<Package>,
}

impl Graph {
    /// Index of the package matching a dependency string.
    ///
    /// A dependency is either `name` (when only one version exists) or `name version`.
    pub fn resolve(&self, dep: &str) -> Option<usize> {
        let mut parts = dep.split_whitespace();
        let name = parts.next()?;
        let version = parts.next();
        self.packages.iter().position(|p| {
            p.name == name && version.is_none_or(|v| p.version == v)
        })
    }

    /// Each package's direct dependencies, as package indices. Unresolvable
    /// entries are dropped.
    pub fn edges(&self) -> Vec<Vec<usize>> {
        self.packages
            .iter()
            .map(|p| p.dependencies.iter().filter_map(|d| self.resolve(d)).collect())
            .collect()
    }
}

/// Parses the text of a `Cargo.lock` file.
///
/// Reads the `[[package]]` tables for `name`, `version`, and the `dependencies`
/// array. Other fields are ignored, so lock-file versions 3 and 4 both parse.
///
/// ```
/// use tpt_dev_dep::parse_lock;
///
/// let g = parse_lock(
///     "[[package]]\nname = \"app\"\nversion = \"1.0.0\"\ndependencies = [\"lib\"]\n\n\
///      [[package]]\nname = \"lib\"\nversion = \"0.1.0\"\n",
/// ).unwrap();
/// assert_eq!(g.packages.len(), 2);
/// assert_eq!(g.edges()[0], vec![1]);
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
            let dep = quoted(line).ok_or_else(|| format!("line {}: bad dependency entry", i + 1))?;
            if let Some(p) = current.as_mut() {
                p.dependencies.push(dep);
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
        } else if let Some(rest) = line.strip_prefix("dependencies = ") {
            if rest.trim() == "[]" {
                continue;
            }
            if rest.trim() == "[" {
                in_deps = true;
            } else {
                return Err(format!("line {}: unsupported dependencies syntax", i + 1));
            }
        }
    }
    if let Some(p) = current.take() {
        packages.push(p);
    }
    if packages.iter().any(|p| p.name.is_empty() || p.version.is_empty()) {
        return Err(String::from("a package is missing its name or version"));
    }
    Ok(Graph { packages })
}

/// The text inside the first pair of double quotes in `s`.
fn quoted(s: &str) -> Option<String> {
    let start = s.find('"')? + 1;
    let end = start + s[start..].find('"')?;
    Some(s[start..end].to_string())
}

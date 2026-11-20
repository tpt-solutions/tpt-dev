//! Reading `cargo metadata --format-version 1` output into a [`Graph`].
//!
//! With a resolve graph (the default), edges come from `resolve.nodes[].deps`,
//! so they are the edges Cargo actually uses, each tagged with its dev, build
//! or normal kind. With `--no-deps` the resolve graph is `null`, and the edges
//! come from each package's declared `dependencies` and are matched by name.

use std::collections::{HashMap, HashSet};

use crate::json::{self, Value};
use crate::lock::{DepKind, Dependency, Graph, Package};

/// Parses the output of `cargo metadata --format-version 1`.
///
/// Returns an error for malformed JSON, for a missing or mistyped field, for an
/// unknown dependency kind, and, when a resolve graph is present, for two
/// packages that share a name and version (edges could not be told apart).
///
/// ```
/// use tpt_dev_dep::{Scope, parse_metadata};
///
/// let text = r#"{
///   "packages": [
///     {"name": "app", "version": "1.0.0", "id": "path+file:///ws/app#1.0.0",
///      "source": null, "dependencies": []},
///     {"name": "lib", "version": "0.1.0", "id": "path+file:///ws/lib#0.1.0",
///      "source": null, "dependencies": []}
///   ],
///   "resolve": {"nodes": [
///     {"id": "path+file:///ws/app#1.0.0", "deps": [
///       {"name": "lib", "pkg": "path+file:///ws/lib#0.1.0",
///        "dep_kinds": [{"kind": null, "target": null}]}
///     ]},
///     {"id": "path+file:///ws/lib#0.1.0", "deps": []}
///   ]}
/// }"#;
/// let g = parse_metadata(text).unwrap();
/// assert_eq!(g.edges(Scope::default())[0], vec![1]);
/// ```
pub fn parse_metadata(text: &str) -> Result<Graph, String> {
    let root = json::parse(text).map_err(|e| e.to_string())?;
    let packages = field_array(&root, "packages")?;
    let resolve = match root.get("resolve") {
        None | Some(Value::Null) => None,
        Some(v @ Value::Object(_)) => Some(v),
        Some(_) => return Err(String::from("\"resolve\" must be an object or null")),
    };
    match resolve {
        None => without_resolve(packages),
        Some(resolve) => with_resolve(packages, resolve),
    }
}

/// Reads each package's `license` field from `cargo metadata --format-version 1`
/// output, as `(name, license-expression)` pairs.
///
/// A package with a null license is left out, so the license check reports it
/// as missing. Use the result as the license list when no `licenses.txt` is given.
///
/// ```
/// use tpt_dev_dep::parse_metadata_licenses;
///
/// let text = r#"{"packages": [
///   {"name": "app", "license": "MIT OR Apache-2.0"},
///   {"name": "bare", "license": null}
/// ]}"#;
/// let pairs = parse_metadata_licenses(text).unwrap();
/// assert_eq!(pairs, vec![(String::from("app"), String::from("MIT OR Apache-2.0"))]);
/// ```
pub fn parse_metadata_licenses(text: &str) -> Result<Vec<(String, String)>, String> {
    let root = json::parse(text).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for pkg in field_array(&root, "packages")? {
        if let Some(license) = field_opt_str(pkg, "license")? {
            out.push((field_str(pkg, "name")?.to_string(), license));
        }
    }
    Ok(out)
}

/// A dependency as declared in a package's manifest.
struct Declared {
    name: String,
    kind: DepKind,
    optional: bool,
}

/// Builds the graph from declared dependencies only (`--no-deps` output).
fn without_resolve(packages: &[Value]) -> Result<Graph, String> {
    let mut out = Vec::with_capacity(packages.len());
    for pkg in packages {
        let declared = declared_deps(pkg)?;
        let dependencies = declared
            .into_iter()
            .map(|d| Dependency {
                target: d.name,
                kind: d.kind,
                optional: d.optional,
            })
            .collect();
        out.push(Package {
            name: field_str(pkg, "name")?.to_string(),
            version: field_str(pkg, "version")?.to_string(),
            source: field_opt_str(pkg, "source")?,
            dependencies,
        });
    }
    Ok(Graph { packages: out })
}

/// Builds the graph from the resolve graph, with each edge's kinds.
fn with_resolve(packages: &[Value], resolve: &Value) -> Result<Graph, String> {
    let mut out: Vec<Package> = Vec::with_capacity(packages.len());
    let mut declared: Vec<Vec<Declared>> = Vec::with_capacity(packages.len());
    let mut index_of_id: HashMap<&str, usize> = HashMap::new();
    let mut seen_versions: HashSet<(&str, &str)> = HashSet::new();

    for (i, pkg) in packages.iter().enumerate() {
        let id = field_str(pkg, "id")?;
        if index_of_id.insert(id, i).is_some() {
            return Err(format!("package id {id} appears twice"));
        }
        let name = field_str(pkg, "name")?;
        let version = field_str(pkg, "version")?;
        if !seen_versions.insert((name, version)) {
            return Err(format!(
                "two packages are named {name} {version}; the resolve graph cannot tell them apart"
            ));
        }
        declared.push(declared_deps(pkg)?);
        out.push(Package {
            name: name.to_string(),
            version: version.to_string(),
            source: field_opt_str(pkg, "source")?,
            dependencies: Vec::new(),
        });
    }

    for node in field_array(resolve, "nodes")? {
        let id = field_str(node, "id")?;
        let from = *index_of_id
            .get(id)
            .ok_or_else(|| format!("resolve node {id} is not in packages"))?;
        for dep in field_array(node, "deps")? {
            let pkg_id = field_str(dep, "pkg")?;
            let to = *index_of_id
                .get(pkg_id)
                .ok_or_else(|| format!("resolve dependency {pkg_id} is not in packages"))?;
            let (target_name, target_version) = match packages.get(to) {
                Some(p) => (field_str(p, "name")?, field_str(p, "version")?),
                None => return Err(format!("resolve dependency {pkg_id} is not in packages")),
            };
            let target = format!("{target_name} {target_version}");
            let kinds = field_array(dep, "dep_kinds")?;
            // An empty list means a plain normal dependency.
            let kinds: Vec<DepKind> = if kinds.is_empty() {
                vec![DepKind::Normal]
            } else {
                kinds
                    .iter()
                    .map(|k| kind_of(k.get("kind")))
                    .collect::<Result<_, _>>()?
            };
            for kind in kinds {
                let optional = declared.get(from).is_some_and(|decls| {
                    decls
                        .iter()
                        .any(|d| d.name == target_name && d.kind == kind && d.optional)
                });
                let edge = Dependency {
                    target: target.clone(),
                    kind,
                    optional,
                };
                if let Some(pkg) = out.get_mut(from)
                    && !pkg
                        .dependencies
                        .iter()
                        .any(|d| d.target == edge.target && d.kind == edge.kind)
                {
                    pkg.dependencies.push(edge);
                }
            }
        }
    }
    Ok(Graph { packages: out })
}

/// The declared dependencies of one package object.
fn declared_deps(pkg: &Value) -> Result<Vec<Declared>, String> {
    field_array(pkg, "dependencies")?
        .iter()
        .map(|d| {
            Ok(Declared {
                name: field_str(d, "name")?.to_string(),
                kind: kind_of(d.get("kind"))?,
                optional: field_bool(d, "optional")?,
            })
        })
        .collect()
}

/// Maps a `kind` value: `null` is normal, and `"dev"` and `"build"` are the others.
fn kind_of(v: Option<&Value>) -> Result<DepKind, String> {
    match v {
        None | Some(Value::Null) => Ok(DepKind::Normal),
        Some(Value::String(s)) if s == "dev" => Ok(DepKind::Dev),
        Some(Value::String(s)) if s == "build" => Ok(DepKind::Build),
        Some(_) => Err(String::from("unknown dependency kind")),
    }
}

fn field_str<'a>(obj: &'a Value, key: &str) -> Result<&'a str, String> {
    obj.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or non-string field \"{key}\""))
}

fn field_opt_str(obj: &Value, key: &str) -> Result<Option<String>, String> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| format!("field \"{key}\" must be a string or null")),
    }
}

fn field_bool(obj: &Value, key: &str) -> Result<bool, String> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| format!("field \"{key}\" must be a boolean")),
    }
}

fn field_array<'a>(obj: &'a Value, key: &str) -> Result<&'a [Value], String> {
    obj.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing or non-array field \"{key}\""))
}

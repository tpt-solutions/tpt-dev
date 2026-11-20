//! The analysis: cycles, license problems, and transitive dependency counts.

use crate::license::Policy;
use crate::lock::{Graph, Scope};

/// Everything the analysis found.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Report {
    /// Dependency cycles, each a sorted list of package names.
    pub cycles: Vec<Vec<String>>,
    /// Packages whose license is not allowed, as `(name, license)`.
    /// A package with no license given is reported with `missing`.
    pub license_violations: Vec<(String, String)>,
    /// Transitive dependency count per package, largest first.
    pub transitive: Vec<(String, usize)>,
}

impl Report {
    /// Returns `true` if there are no cycles and no license violations.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_dep::Report;
    ///
    /// assert!(Report::default().is_clean());
    ///
    /// let dirty = Report {
    ///     cycles: vec![vec!["a".to_string(), "b".to_string()]],
    ///     ..Default::default()
    /// };
    /// assert!(!dirty.is_clean());
    /// ```
    pub fn is_clean(&self) -> bool {
        self.cycles.is_empty() && self.license_violations.is_empty()
    }
}

/// Analyses `graph`, following the edge kinds in `scope`. `licenses` maps package
/// names to license expressions.
///
/// ```
/// use tpt_dev_dep::{Policy, Scope, analyse, parse_lock};
///
/// let g = parse_lock(
///     "[[package]]\nname = \"a\"\nversion = \"1.0.0\"\ndependencies = [\"b\"]\n\n\
///      [[package]]\nname = \"b\"\nversion = \"1.0.0\"\ndependencies = [\"a\"]\n",
/// ).unwrap();
/// let policy = Policy { allow: vec!["MIT".into()] };
/// let r = analyse(&g, &[("a".into(), "MIT".into()), ("b".into(), "MIT".into())], &policy, Scope::default());
/// assert_eq!(r.cycles.len(), 1);
/// assert!(!r.is_clean());
/// ```
pub fn analyse(
    graph: &Graph,
    licenses: &[(String, String)],
    policy: &Policy,
    scope: Scope,
) -> Report {
    let edges = graph.edges(scope);
    let cycles = cycles(graph, &edges);

    // A package pulled in only through an edge outside `scope` (for example a
    // dev-dependency's subtree) is not part of the analysed graph, so its
    // license is not checked. Packages no root reaches at all stay checked.
    let all_edges = graph.edges(Scope::ALL);
    let mut has_incoming = vec![false; graph.packages.len()];
    for targets in &all_edges {
        for &t in targets {
            has_incoming[t] = true;
        }
    }
    let roots: Vec<usize> = (0..graph.packages.len())
        .filter(|&i| !has_incoming[i])
        .collect();
    let in_scope = reached(&edges, &roots);
    let in_all = reached(&all_edges, &roots);

    let mut license_violations = Vec::new();
    for (i, p) in graph.packages.iter().enumerate() {
        if !(in_scope[i] || !in_all[i]) {
            continue;
        }
        let license = licenses
            .iter()
            .find(|(n, _)| *n == p.name)
            .map(|(_, l)| l.as_str());
        match license {
            None => license_violations.push((p.name.clone(), String::from("missing"))),
            Some(l) if !policy.accepts(l) => {
                license_violations.push((p.name.clone(), l.to_string()))
            }
            Some(_) => {}
        }
    }

    let mut transitive: Vec<(String, usize)> = (0..graph.packages.len())
        .map(|i| {
            (
                graph.packages[i].name.clone(),
                reachable(&edges, i).saturating_sub(1),
            )
        })
        .collect();
    transitive.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    Report {
        cycles,
        license_violations,
        transitive,
    }
}

/// For each node, whether it is reachable from any of `roots`.
fn reached(edges: &[Vec<usize>], roots: &[usize]) -> Vec<bool> {
    let mut seen = vec![false; edges.len()];
    let mut stack: Vec<usize> = roots.to_vec();
    for &r in roots {
        seen[r] = true;
    }
    while let Some(v) = stack.pop() {
        for &w in &edges[v] {
            if !seen[w] {
                seen[w] = true;
                stack.push(w);
            }
        }
    }
    seen
}

/// Number of nodes reachable from `start`, including `start`.
fn reachable(edges: &[Vec<usize>], start: usize) -> usize {
    let mut seen = vec![false; edges.len()];
    let mut stack = vec![start];
    seen[start] = true;
    let mut count = 0;
    while let Some(v) = stack.pop() {
        count += 1;
        for &w in &edges[v] {
            if !seen[w] {
                seen[w] = true;
                stack.push(w);
            }
        }
    }
    count
}

/// Strongly connected components that contain a cycle, by Tarjan's algorithm.
fn cycles(graph: &Graph, edges: &[Vec<usize>]) -> Vec<Vec<String>> {
    struct State {
        index: usize,
        indices: Vec<Option<usize>>,
        low: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        out: Vec<Vec<usize>>,
    }
    fn visit(v: usize, edges: &[Vec<usize>], st: &mut State) {
        st.indices[v] = Some(st.index);
        st.low[v] = st.index;
        st.index += 1;
        st.stack.push(v);
        st.on_stack[v] = true;
        for &w in &edges[v] {
            match st.indices[w] {
                None => {
                    visit(w, edges, st);
                    st.low[v] = st.low[v].min(st.low[w]);
                }
                Some(iw) if st.on_stack[w] => st.low[v] = st.low[v].min(iw),
                Some(_) => {}
            }
        }
        if Some(st.low[v]) == st.indices[v] {
            let mut comp = Vec::new();
            while let Some(w) = st.stack.pop() {
                st.on_stack[w] = false;
                comp.push(w);
                if w == v {
                    break;
                }
            }
            let self_loop = comp.len() == 1 && edges[v].contains(&v);
            if comp.len() > 1 || self_loop {
                st.out.push(comp);
            }
        }
    }
    let n = edges.len();
    let mut st = State {
        index: 0,
        indices: vec![None; n],
        low: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        out: Vec::new(),
    };
    for v in 0..n {
        if st.indices[v].is_none() {
            visit(v, edges, &mut st);
        }
    }
    st.out
        .into_iter()
        .map(|comp| {
            let mut names: Vec<String> = comp
                .into_iter()
                .map(|i| graph.packages[i].name.clone())
                .collect();
            names.sort();
            names
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::{DepKind, Dependency, Package};

    fn pkg(name: &str, deps: Vec<Dependency>) -> Package {
        Package {
            name: name.to_string(),
            version: String::from("1.0.0"),
            source: None,
            dependencies: deps,
        }
    }

    fn dep(target: &str, kind: DepKind) -> Dependency {
        Dependency {
            target: target.to_string(),
            kind,
            optional: false,
        }
    }

    /// `app` uses `lib` normally and `devonly` as a dev-dependency. `devonly`
    /// pulls in `apache`, whose license is not allowed.
    fn graph() -> Graph {
        Graph {
            packages: vec![
                pkg(
                    "app",
                    vec![dep("lib", DepKind::Normal), dep("devonly", DepKind::Dev)],
                ),
                pkg("lib", vec![]),
                pkg("devonly", vec![dep("apache", DepKind::Normal)]),
                pkg("apache", vec![]),
            ],
        }
    }

    fn licenses() -> Vec<(String, String)> {
        [
            ("app", "MIT"),
            ("lib", "MIT"),
            ("devonly", "MIT"),
            ("apache", "Apache-2.0"),
        ]
        .iter()
        .map(|(n, l)| (n.to_string(), l.to_string()))
        .collect()
    }

    fn policy() -> Policy {
        Policy {
            allow: vec![String::from("MIT")],
        }
    }

    #[test]
    fn dev_only_subtree_license_is_not_checked_by_default() {
        let r = analyse(&graph(), &licenses(), &policy(), Scope::default());
        assert!(
            r.license_violations.is_empty(),
            "{:?}",
            r.license_violations
        );
    }

    #[test]
    fn dev_only_subtree_license_is_checked_when_dev_is_included() {
        let r = analyse(&graph(), &licenses(), &policy(), Scope::ALL);
        assert_eq!(
            r.license_violations,
            vec![(String::from("apache"), String::from("Apache-2.0"))]
        );
    }

    #[test]
    fn normal_dependency_license_is_always_checked() {
        let mut lic = licenses();
        lic.retain(|(n, _)| n != "lib");
        lic.push((String::from("lib"), String::from("GPL-3.0")));
        let r = analyse(&graph(), &lic, &policy(), Scope::default());
        assert_eq!(
            r.license_violations,
            vec![(String::from("lib"), String::from("GPL-3.0"))]
        );
    }
}

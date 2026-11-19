//! The analysis: cycles, license problems, and transitive dependency counts.

use crate::license::Policy;
use crate::lock::Graph;

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
    pub fn is_clean(&self) -> bool {
        self.cycles.is_empty() && self.license_violations.is_empty()
    }
}

/// Analyses `graph`. `licenses` maps package names to license expressions.
///
/// ```
/// use tpt_dev_dep::{Policy, analyse, parse_lock};
///
/// let g = parse_lock(
///     "[[package]]\nname = \"a\"\nversion = \"1.0.0\"\ndependencies = [\"b\"]\n\n\
///      [[package]]\nname = \"b\"\nversion = \"1.0.0\"\ndependencies = [\"a\"]\n",
/// ).unwrap();
/// let policy = Policy { allow: vec!["MIT".into()] };
/// let r = analyse(&g, &[("a".into(), "MIT".into()), ("b".into(), "MIT".into())], &policy);
/// assert_eq!(r.cycles.len(), 1);
/// assert!(!r.is_clean());
/// ```
pub fn analyse(graph: &Graph, licenses: &[(String, String)], policy: &Policy) -> Report {
    let edges = graph.edges();
    let cycles = cycles(graph, &edges);

    let mut license_violations = Vec::new();
    for p in &graph.packages {
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

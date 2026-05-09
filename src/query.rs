use crate::model::{CodeGraph, Edge, EdgeKind, Node, NodeKind};
use anyhow::{Context, Result};
use petgraph::algo::astar;
use petgraph::graphmap::DiGraphMap;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, VecDeque};

fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.chars().collect::<Vec<_>>();
    let b = b.chars().collect::<Vec<_>>();
    let n = a.len();
    let m = b.len();
    let mut prev = (0..=m).collect::<Vec<_>>();
    let mut curr = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

fn suggest(query: &str, candidates: &[&str], limit: usize) -> String {
    let mut scored: Vec<(&str, usize)> = candidates
        .iter()
        .map(|c| (*c, levenshtein(query, c)))
        .collect();
    scored.sort_by_key(|(_, d)| *d);
    let suggestions: Vec<_> = scored
        .iter()
        .take(limit)
        .filter(|(_, d)| *d < query.len().max(5))
        .map(|(name, _)| format!("  • {name}"))
        .collect();
    if suggestions.is_empty() {
        String::new()
    } else {
        format!("\nDid you mean?\n{}", suggestions.join("\n"))
    }
}

struct QueryIndex<'a> {
    nodes_by_id: HashMap<&'a str, &'a Node>,
    outbound: HashMap<&'a str, Vec<&'a Edge>>,
    inbound: HashMap<&'a str, Vec<&'a Edge>>,
    degree: HashMap<&'a str, usize>,
}

impl<'a> QueryIndex<'a> {
    fn new(graph: &'a CodeGraph) -> Self {
        let mut nodes_by_id = HashMap::with_capacity(graph.nodes.len());
        let mut outbound = HashMap::<&str, Vec<&Edge>>::new();
        let mut inbound = HashMap::<&str, Vec<&Edge>>::new();
        let mut degree = HashMap::<&str, usize>::new();
        for node in &graph.nodes {
            nodes_by_id.insert(node.id.as_str(), node);
        }
        for edge in &graph.edges {
            outbound.entry(edge.from.as_str()).or_default().push(edge);
            inbound.entry(edge.to.as_str()).or_default().push(edge);
            *degree.entry(edge.from.as_str()).or_default() += edge.weight;
            *degree.entry(edge.to.as_str()).or_default() += edge.weight;
        }
        Self {
            nodes_by_id,
            outbound,
            inbound,
            degree,
        }
    }

    fn edges(&self, id: &str, outbound: bool) -> &[&'a Edge] {
        if outbound {
            self.outbound.get(id)
        } else {
            self.inbound.get(id)
        }
        .map(Vec::as_slice)
        .unwrap_or(&[])
    }

    fn node(&self, id: &str) -> Option<&'a Node> {
        self.nodes_by_id.get(id).copied()
    }

    fn degree(&self, id: &str) -> usize {
        self.degree.get(id).copied().unwrap_or_default()
    }
}

pub fn summary(graph: &CodeGraph, limit: usize) -> Value {
    let index = QueryIndex::new(graph);
    let mut files = graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .map(|node| {
            let outbound = index.edges(&node.id, true).len();
            let inbound = index.edges(&node.id, false).len();
            json!({
                "path": node.name,
                "lines": node.metrics.get("lines").copied().unwrap_or_default(),
                "outbound": outbound,
                "inbound": inbound,
                "symbols": index.edges(&node.id, true).iter().filter(|edge| edge.kind == EdgeKind::ModuleFile).count()
            })
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|file| {
        std::cmp::Reverse(
            file.get("outbound")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                + file
                    .get("inbound")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
        )
    });
    json!({
        "kind": "summary",
        "project": graph.project,
        "stats": graph.stats(),
        "top_files": files.into_iter().take(limit).collect::<Vec<_>>(),
        "hot_symbols": hot_symbols(graph, &index, limit)
    })
}

pub fn symbols(graph: &CodeGraph, query: Option<&str>, kind: Option<&str>, limit: usize) -> Value {
    let index = QueryIndex::new(graph);
    json!({
        "kind": "symbols",
        "items": ranked_nodes(graph, &index, query.unwrap_or(""), limit)
            .into_iter()
            .filter(|node| kind.is_none_or(|kind| node.kind.as_str() == kind))
            .map(|node| node_value(&index, node))
            .collect::<Vec<_>>()
    })
}

pub fn symbol(graph: &CodeGraph, name: &str) -> Result<Value> {
    let index = QueryIndex::new(graph);
    let matches = find_nodes(graph, name);
    if matches.is_empty() {
        let names: Vec<&str> = graph.nodes.iter().map(|n| n.name.as_str()).collect();
        anyhow::bail!("symbol `{name}` not found{}", suggest(name, &names, 3));
    }
    if matches.len() == 1 {
        let node = matches[0];
        return Ok(json!({
            "kind": "symbol",
            "node": node_value(&index, node),
            "incoming": adjacent(&index, &node.id, false, None, 100),
            "outgoing": adjacent(&index, &node.id, true, None, 100)
        }));
    }
    Ok(json!({
        "kind": "ambiguous",
        "name": name,
        "matches": matches.iter().map(|node| json!({
            "id": node.id,
            "name": node.name,
            "qualified_name": node.qualified_name,
            "kind": node.kind.as_str(),
            "file": node.file,
            "range": node.range
        })).collect::<Vec<_>>()
    }))
}

pub fn file(graph: &CodeGraph, path: &str) -> Result<Value> {
    let index = QueryIndex::new(graph);
    let node = graph
        .nodes
        .iter()
        .find(|node| {
            node.kind == NodeKind::File
                && (node.name == path
                    || node.name.ends_with(path)
                    || node.qualified_name.ends_with(path))
        })
        .with_context(|| {
            let files: Vec<&str> = graph.nodes.iter().filter(|n| n.kind == NodeKind::File).map(|n| n.name.as_str()).collect();
            format!("file `{path}` not found{}", suggest(path, &files, 3))
        })?;
    let declares = index
        .edges(&node.id, true)
        .iter()
        .find(|edge| edge.kind == EdgeKind::ModuleFile)
        .and_then(|edge| {
            index.node(&edge.to).map(|module| {
                adjacent(&index, &module.id, true, Some("declares"), 500)
            })
        })
        .unwrap_or_default();
    Ok(json!({
        "kind": "file",
        "node": node_value(&index, node),
        "declares": declares,
        "module": adjacent(&index, &node.id, true, Some("module_file"), 20),
        "incoming": adjacent(&index, &node.id, false, None, 100),
        "outgoing": adjacent(&index, &node.id, true, None, 100)
    }))
}

pub fn module(graph: &CodeGraph, name: &str) -> Result<Value> {
    let index = QueryIndex::new(graph);
    let matches = find_nodes(graph, name)
        .into_iter()
        .filter(|node| node.kind == NodeKind::Module)
        .collect::<Vec<_>>();
    let node = if matches.is_empty() {
        None
    } else {
        matches
            .iter()
            .copied()
            .find(|candidate| !adjacent(&index, &candidate.id, true, Some("declares"), 1).is_empty())
            .or_else(|| matches.first().copied())
    }
    .with_context(|| {
        let mods: Vec<&str> = graph.nodes.iter().filter(|n| n.kind == NodeKind::Module).map(|n| n.name.as_str()).collect();
        format!("module `{name}` not found{}", suggest(name, &mods, 3))
    })?;
    Ok(json!({
        "kind": "module",
        "node": node_value(&index, node),
        "declares": adjacent(&index, &node.id, true, Some("declares"), 500),
        "imports": adjacent(&index, &node.id, true, Some("imports"), 200),
        "incoming": adjacent(&index, &node.id, false, None, 100)
    }))
}

pub fn neighbors(
    graph: &CodeGraph,
    name: &str,
    edge_kind: &str,
    outbound: bool,
    depth: usize,
    limit: usize,
) -> Result<Value> {
    let index = QueryIndex::new(graph);
    let node = require_unique_node(graph, name, "symbol")?;
    Ok(json!({
        "kind": if outbound { "callees" } else { "callers" },
        "root": node_value(&index, node),
        "depth": depth,
        "items": walk(&index, &node.id, outbound, Some(edge_kind), depth, limit)
    }))
}

pub fn impact(graph: &CodeGraph, name: &str, depth: usize, limit: usize) -> Result<Value> {
    let index = QueryIndex::new(graph);
    let node = require_unique_node(graph, name, "symbol")?;
    Ok(json!({
        "kind": "impact",
        "root": node_value(&index, node),
        "callers": walk(&index, &node.id, false, Some("calls"), depth, limit),
        "dependents": walk(&index, &node.id, false, None, depth, limit),
        "dependencies": walk(&index, &node.id, true, None, depth, limit)
    }))
}

pub fn search(graph: &CodeGraph, query: &str, limit: usize) -> Value {
    let index = QueryIndex::new(graph);
    json!({
        "kind": "search",
        "query": query,
        "items": ranked_nodes(graph, &index, query, limit)
            .into_iter()
            .map(|node| node_value(&index, node))
            .collect::<Vec<_>>()
    })
}

pub fn path(graph: &CodeGraph, from: &str, to: &str, max_depth: usize) -> Result<Value> {
    let index = QueryIndex::new(graph);
    let from = require_unique_node(graph, from, "source")?;
    let to = require_unique_node(graph, to, "target")?;
    let mut map = DiGraphMap::<&str, usize>::new();
    for edge in &graph.edges {
        map.add_edge(edge.from.as_str(), edge.to.as_str(), 1);
    }
    let result = astar(
        &map,
        from.id.as_str(),
        |node| node == to.id,
        |edge| *edge.2,
        |_| 0,
    );
    let nodes = result
        .map(|(_, path)| path)
        .filter(|path| path.len().saturating_sub(1) <= max_depth)
        .unwrap_or_default();
    Ok(json!({
        "kind": "path",
        "from": node_value(&index, from),
        "to": node_value(&index, to),
        "found": !nodes.is_empty(),
        "nodes": nodes
            .iter()
            .filter_map(|id| index.node(id))
            .map(|node| node_value(&index, node))
            .collect::<Vec<_>>()
    }))
}

fn ranked_nodes<'a>(
    graph: &'a CodeGraph,
    index: &QueryIndex<'a>,
    query: &str,
    limit: usize,
) -> Vec<&'a Node> {
    let terms = query
        .to_lowercase()
        .split(|char: char| !char.is_ascii_alphanumeric() && char != '_')
        .filter(|term| !term.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut nodes = graph
        .nodes
        .iter()
        .filter(|node| !matches!(node.kind, NodeKind::Project))
        .map(|node| {
            let haystack = format!(
                "{} {} {} {}",
                node.name,
                node.qualified_name,
                node.file.as_deref().unwrap_or_default(),
                node.docs.as_deref().unwrap_or_default()
            )
            .to_lowercase();
            let text_score = if terms.is_empty() {
                1
            } else {
                terms
                    .iter()
                    .map(|term| {
                        if haystack == *term {
                            100
                        } else if haystack.contains(term) {
                            20
                        } else {
                            0
                        }
                    })
                    .sum()
            };
            if !terms.is_empty() && text_score == 0 {
                return (0, node);
            }
            let degree = index.degree(&node.id);
            (text_score + degree, node)
        })
        .filter(|(score, _)| *score > 0)
        .collect::<Vec<_>>();
    nodes.sort_by_key(|(score, node)| {
        (
            std::cmp::Reverse(*score),
            node.kind.as_str().to_string(),
            node.qualified_name.clone(),
        )
    });
    nodes
        .into_iter()
        .map(|(_, node)| node)
        .take(limit)
        .collect()
}

fn find_nodes<'a>(graph: &'a CodeGraph, name: &str) -> Vec<&'a Node> {
    let exact = graph
        .nodes
        .iter()
        .filter(|node| {
            node.id == name
                || node.id.strip_suffix(|ch: char| ch == '#' || ch.is_ascii_digit()).is_some_and(|base| base == name)
                || node.qualified_name == name
        })
        .collect::<Vec<_>>();
    if !exact.is_empty() {
        return exact;
    }

    let by_name = graph
        .nodes
        .iter()
        .filter(|node| node.name == name)
        .collect::<Vec<_>>();
    if !by_name.is_empty() {
        return by_name;
    }

    let suffix = format!("::{name}");
    graph
        .nodes
        .iter()
        .filter(|node| node.qualified_name.ends_with(&suffix))
        .collect()
}

#[allow(dead_code)]
fn find_node<'a>(graph: &'a CodeGraph, name: &str) -> Option<&'a Node> {
    find_nodes(graph, name).into_iter().next()
}

fn require_unique_node<'a>(graph: &'a CodeGraph, name: &str, label: &str) -> Result<&'a Node> {
    let matches = find_nodes(graph, name);
    if matches.is_empty() {
        let names: Vec<&str> = graph.nodes.iter().map(|n| n.name.as_str()).collect();
        anyhow::bail!("{label} `{name}` not found{}", suggest(name, &names, 3));
    }
    if matches.len() > 1 {
        let names = matches
            .iter()
            .map(|node| node.qualified_name.as_str())
            .collect::<Vec<_>>();
        anyhow::bail!("{label} `{name}` is ambiguous, matches: {}", names.join(", "));
    }
    Ok(matches[0])
}

fn adjacent(
    index: &QueryIndex,
    id: &str,
    outbound: bool,
    kind: Option<&str>,
    limit: usize,
) -> Vec<Value> {
    index
        .edges(id, outbound)
        .iter()
        .copied()
        .filter(|edge| kind.is_none_or(|kind| edge.kind.as_str() == kind))
        .filter_map(|edge| {
            let other = if outbound { &edge.to } else { &edge.from };
            index.node(other).map(|node| {
                json!({
                    "edge": edge,
                    "node": node_value(index, node)
                })
            })
        })
        .take(limit)
        .collect()
}

fn walk(
    index: &QueryIndex,
    start: &str,
    outbound: bool,
    kind: Option<&str>,
    depth: usize,
    limit: usize,
) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([(start.to_string(), 0usize)]);
    let mut result = Vec::new();
    while let Some((id, level)) = queue.pop_front() {
        if level >= depth || result.len() >= limit {
            continue;
        }
        for edge in index
            .edges(&id, outbound)
            .iter()
            .copied()
            .filter(|edge| kind.is_none_or(|kind| edge.kind.as_str() == kind))
        {
            let other = if outbound { &edge.to } else { &edge.from };
            if !seen.insert(other.clone()) {
                continue;
            }
            if let Some(node) = index.node(other) {
                result.push(json!({
                    "depth": level + 1,
                    "edge": edge,
                    "node": node_value(index, node)
                }));
                queue.push_back((other.clone(), level + 1));
            }
        }
    }
    result
}

fn hot_symbols(graph: &CodeGraph, index: &QueryIndex, limit: usize) -> Vec<Value> {
    let mut items = graph
        .nodes
        .iter()
        .filter(|node| {
            !matches!(
                node.kind,
                NodeKind::Project | NodeKind::Crate | NodeKind::File | NodeKind::Module
            )
        })
        .map(|node| (index.degree(&node.id), node))
        .collect::<Vec<_>>();
    items.sort_by_key(|(degree, node)| (std::cmp::Reverse(*degree), node.qualified_name.clone()));
    items
        .into_iter()
        .take(limit)
        .map(|(degree, node)| {
            let mut value = node_value(index, node);
            value["degree"] = json!(degree);
            value
        })
        .collect()
}

fn node_value(index: &QueryIndex, node: &Node) -> Value {
    json!({
        "id": node.id,
        "kind": node.kind.as_str(),
        "name": node.name,
        "qualified_name": node.qualified_name,
        "file": node.file,
        "range": node.range,
        "visibility": node.visibility,
        "signature": node.signature,
        "docs": node.docs,
        "degree": index.degree(&node.id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BuildProfile, EdgeCertainty, EdgeSource, Package, Project, Range, Target};
    use std::collections::BTreeMap;

    #[test]
    fn symbol_reports_ambiguous_short_names() {
        let graph = ambiguous_graph();

        let result = symbol(&graph, "index_project").unwrap();

        assert_eq!(result["kind"], "ambiguous");
        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches
            .iter()
            .any(|node| node["qualified_name"] == "ferrimind::index_project"));
        assert!(matches
            .iter()
            .any(|node| node["qualified_name"] == "ferrimind::analyzer::index_project"));
    }

    #[test]
    fn symbol_prefers_exact_qualified_name_match() {
        let graph = ambiguous_graph();

        let result = symbol(&graph, "ferrimind::index_project").unwrap();

        assert_eq!(result["kind"], "symbol");
        assert_eq!(result["node"]["qualified_name"], "ferrimind::index_project");
    }

    #[test]
    fn neighbors_fail_on_ambiguous_short_names() {
        let graph = ambiguous_graph();

        let error = neighbors(&graph, "index_project", "calls", true, 1, 10).unwrap_err();

        assert_eq!(
            error.to_string(),
            "symbol `index_project` is ambiguous, matches: ferrimind::index_project, ferrimind::analyzer::index_project"
        );
    }

    #[test]
    fn path_fails_when_source_symbol_is_ambiguous() {
        let graph = ambiguous_graph();

        let error = path(&graph, "index_project", "helper", 3).unwrap_err();

        assert_eq!(
            error.to_string(),
            "source `index_project` is ambiguous, matches: ferrimind::index_project, ferrimind::analyzer::index_project"
        );
    }

    #[test]
    fn file_lists_symbols_declared_in_matching_path() {
        let graph = file_and_module_graph();

        let result = file(&graph, "src/main.rs").unwrap();
        let declares = result["declares"].as_array().unwrap();

        assert_eq!(declares.len(), 3);
        assert!(declares.iter().any(|item| item["node"]["qualified_name"] == "ferrimind::model"));
        assert!(declares.iter().any(|item| item["node"]["qualified_name"] == "ferrimind::main_fn"));
        assert!(declares.iter().any(|item| item["node"]["qualified_name"] == "ferrimind::App"));
    }

    #[test]
    fn module_prefers_declaring_module_node_with_matching_qualified_name() {
        let graph = file_and_module_graph();

        let result = module(&graph, "ferrimind::model").unwrap();
        let declares = result["declares"].as_array().unwrap();

        assert_eq!(result["node"]["id"], "module:ferrimind::model#2");
        assert_eq!(declares.len(), 1);
        assert_eq!(declares[0]["node"]["qualified_name"], "ferrimind::model::Node");
    }

    fn ambiguous_graph() -> CodeGraph {
        CodeGraph {
            schema_version: 1,
            project: project(),
            nodes: vec![
                node(
                    "function:ferrimind::index_project",
                    NodeKind::Function,
                    "index_project",
                    "ferrimind::index_project",
                    "src/main.rs",
                    10,
                ),
                node(
                    "function:ferrimind::analyzer::index_project",
                    NodeKind::Function,
                    "index_project",
                    "ferrimind::analyzer::index_project",
                    "src/analyzer.rs",
                    30,
                ),
                node(
                    "function:ferrimind::helper",
                    NodeKind::Function,
                    "helper",
                    "ferrimind::helper",
                    "src/main.rs",
                    50,
                ),
            ],
            edges: vec![Edge {
                from: "function:ferrimind::index_project".to_string(),
                to: "function:ferrimind::helper".to_string(),
                kind: EdgeKind::Calls,
                label: None,
                evidence: None,
                weight: 1,
                source: EdgeSource::Ast,
                certainty: EdgeCertainty::Definite,
                profiles: vec![],
            }],
            warnings: vec![],
            semantic: None,
            mir: None,
            profiles: vec![profile()],
            generated_at_ms: 0,
        }
    }

    fn file_and_module_graph() -> CodeGraph {
        CodeGraph {
            schema_version: 1,
            project: project(),
            nodes: vec![
                node(
                    "file:src/main.rs",
                    NodeKind::File,
                    "src/main.rs",
                    "src/main.rs",
                    "src/main.rs",
                    1,
                ),
                node(
                    "module:codegraph",
                    NodeKind::Module,
                    "codegraph",
                    "codegraph",
                    "src/main.rs",
                    1,
                ),
                node(
                    "module:ferrimind::model",
                    NodeKind::Module,
                    "model",
                    "ferrimind::model",
                    "src/main.rs",
                    12,
                ),
                node(
                    "module:ferrimind::model#2",
                    NodeKind::Module,
                    "ferrimind::model",
                    "ferrimind::model",
                    "src/model.rs",
                    1,
                ),
                node(
                    "function:ferrimind::main_fn",
                    NodeKind::Function,
                    "main_fn",
                    "ferrimind::main_fn",
                    "src/main.rs",
                    20,
                ),
                node(
                    "struct:ferrimind::App",
                    NodeKind::Struct,
                    "App",
                    "ferrimind::App",
                    "src/main.rs",
                    30,
                ),
                node(
                    "struct:ferrimind::model::Node",
                    NodeKind::Struct,
                    "Node",
                    "ferrimind::model::Node",
                    "src/model.rs",
                    5,
                ),
            ],
            edges: vec![
                Edge {
                    from: "file:src/main.rs".to_string(),
                    to: "module:codegraph".to_string(),
                    kind: EdgeKind::ModuleFile,
                    label: None,
                    evidence: None,
                    weight: 1,
                    source: EdgeSource::Ast,
                    certainty: EdgeCertainty::Definite,
                    profiles: vec![],
                },
                Edge {
                    from: "module:codegraph".to_string(),
                    to: "module:ferrimind::model".to_string(),
                    kind: EdgeKind::Declares,
                    label: None,
                    evidence: None,
                    weight: 1,
                    source: EdgeSource::Ast,
                    certainty: EdgeCertainty::Definite,
                    profiles: vec![],
                },
                Edge {
                    from: "module:codegraph".to_string(),
                    to: "function:ferrimind::main_fn".to_string(),
                    kind: EdgeKind::Declares,
                    label: None,
                    evidence: None,
                    weight: 1,
                    source: EdgeSource::Ast,
                    certainty: EdgeCertainty::Definite,
                    profiles: vec![],
                },
                Edge {
                    from: "module:codegraph".to_string(),
                    to: "struct:ferrimind::App".to_string(),
                    kind: EdgeKind::Declares,
                    label: None,
                    evidence: None,
                    weight: 1,
                    source: EdgeSource::Ast,
                    certainty: EdgeCertainty::Definite,
                    profiles: vec![],
                },
                Edge {
                    from: "module:ferrimind::model#2".to_string(),
                    to: "struct:ferrimind::model::Node".to_string(),
                    kind: EdgeKind::Declares,
                    label: None,
                    evidence: None,
                    weight: 1,
                    source: EdgeSource::Ast,
                    certainty: EdgeCertainty::Definite,
                    profiles: vec![],
                },
            ],
            warnings: vec![],
            semantic: None,
            mir: None,
            profiles: vec![profile()],
            generated_at_ms: 0,
        }
    }

    fn project() -> Project {
        Project {
            root: "/tmp/demo".to_string(),
            workspace_root: "/tmp/demo".to_string(),
            packages: vec![Package {
                name: "codegraph".to_string(),
                manifest_path: "/tmp/demo/Cargo.toml".to_string(),
                targets: vec![Target {
                    name: "codegraph".to_string(),
                    kind: vec!["bin".to_string()],
                    src_path: "src/main.rs".to_string(),
                }],
            }],
        }
    }

    fn profile() -> BuildProfile {
        BuildProfile {
            name: "default".to_string(),
            features: vec![],
            all_features: false,
            no_default_features: false,
            target: None,
        }
    }

    fn node(
        id: &str,
        kind: NodeKind,
        name: &str,
        qualified_name: &str,
        file: &str,
        line: usize,
    ) -> Node {
        Node {
            id: id.to_string(),
            kind,
            name: name.to_string(),
            qualified_name: qualified_name.to_string(),
            file: Some(file.to_string()),
            range: Some(Range {
                start_line: line,
                end_line: line,
            }),
            visibility: Some("pub".to_string()),
            signature: Some(format!("fn {name}()")),
            docs: None,
            metrics: BTreeMap::new(),
        }
    }
}

use crate::model::{CodeGraph, Edge, EdgeKind, Node, NodeKind};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

struct AiIndex<'a> {
    nodes_by_id: HashMap<&'a str, &'a Node>,
    outbound: HashMap<&'a str, Vec<&'a Edge>>,
    inbound: HashMap<&'a str, Vec<&'a Edge>>,
    degree: HashMap<&'a str, usize>,
}

impl<'a> AiIndex<'a> {
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

    fn node(&self, id: &str) -> Option<&'a Node> {
        self.nodes_by_id.get(id).copied()
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

    fn degree(&self, id: &str) -> usize {
        self.degree.get(id).copied().unwrap_or_default()
    }
}

pub fn quality(graph: &CodeGraph) -> Value {
    let stats = graph.stats();
    let definite = stats
        .by_certainty
        .get("definite")
        .copied()
        .unwrap_or_default();
    let confirmed = stats
        .by_certainty
        .get("confirmed")
        .copied()
        .unwrap_or_default();
    let inferred = stats
        .by_certainty
        .get("inferred")
        .copied()
        .unwrap_or_default();
    let possible = stats
        .by_certainty
        .get("possible")
        .copied()
        .unwrap_or_default();
    let trusted = definite + confirmed;
    let scored_edges = trusted + inferred + possible;
    let confidence = if scored_edges == 0 {
        0
    } else {
        ((trusted * 100) + (inferred * 55) + (possible * 25)) / scored_edges
    };
    let semantic_enabled = graph
        .semantic
        .as_ref()
        .map(|info| info.enabled)
        .unwrap_or_default();
    let mir_enabled = graph
        .mir
        .as_ref()
        .map(|info| info.enabled)
        .unwrap_or_default();
    json!({
        "kind": "quality",
        "score": confidence,
        "interpretation": quality_label(confidence),
        "stats": stats,
        "trusted_edges": trusted,
        "uncertain_edges": inferred + possible,
        "warnings": graph.warnings,
        "recommendations": quality_recommendations(graph, confidence, semantic_enabled, mir_enabled, possible)
    })
}

pub fn entries(graph: &CodeGraph, limit: usize) -> Value {
    let index = AiIndex::new(graph);
    json!({
        "kind": "entries",
        "items": entry_candidates(graph, &index, limit)
    })
}

pub fn clusters(graph: &CodeGraph, limit: usize) -> Value {
    let index = AiIndex::new(graph);
    let mut clusters = BTreeMap::<String, Cluster>::new();
    for node in &graph.nodes {
        let Some(file) = node.file.as_deref() else {
            continue;
        };
        let key = cluster_key(file);
        let cluster = clusters.entry(key.clone()).or_insert_with(|| Cluster {
            name: key,
            files: BTreeSet::new(),
            nodes: 0,
            symbols: 0,
            degree: 0,
            hot: Vec::new(),
        });
        cluster.files.insert(file.to_string());
        cluster.nodes += 1;
        if !matches!(
            node.kind,
            NodeKind::File | NodeKind::Module | NodeKind::Project | NodeKind::Crate
        ) {
            cluster.symbols += 1;
        }
        let degree = index.degree(&node.id);
        cluster.degree += degree;
        if !matches!(
            node.kind,
            NodeKind::File | NodeKind::Module | NodeKind::Project | NodeKind::Crate
        ) {
            cluster.hot.push((degree, node));
        }
    }
    let mut items = clusters
        .into_values()
        .map(|mut cluster| {
            cluster.hot.sort_by_key(|(degree, node)| {
                (std::cmp::Reverse(*degree), node.qualified_name.clone())
            });
            json!({
                "name": cluster.name,
                "files": cluster.files.len(),
                "nodes": cluster.nodes,
                "symbols": cluster.symbols,
                "degree": cluster.degree,
                "hot_symbols": cluster.hot.into_iter().take(8).map(|(_, node)| node_value(&index, node)).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|item| {
        std::cmp::Reverse(
            item.get("degree")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        )
    });
    json!({
        "kind": "clusters",
        "items": items.into_iter().take(limit).collect::<Vec<_>>()
    })
}

pub fn guide(graph: &CodeGraph, query: Option<&str>, depth: usize, limit: usize) -> Value {
    let index = AiIndex::new(graph);
    let roots = query
        .filter(|query| !query.trim().is_empty())
        .map(|query| search_nodes(graph, &index, query, 6))
        .unwrap_or_else(|| {
            entry_candidates_raw(graph, &index, 6)
                .into_iter()
                .map(|(_, node, _)| node)
                .collect()
        });
    json!({
        "kind": "guide",
        "query": query,
        "root_source": "lexical",
        "read_order": read_order(&index, &roots, limit),
        "roots": roots.iter().map(|node| node_value(&index, node)).collect::<Vec<_>>(),
        "callers": roots.iter().flat_map(|node| walk(&index, &node.id, false, Some(EdgeKind::Calls), depth, limit)).collect::<Vec<_>>(),
        "callees": roots.iter().flat_map(|node| walk(&index, &node.id, true, Some(EdgeKind::Calls), depth, limit)).collect::<Vec<_>>(),
        "impact": roots.iter().flat_map(|node| walk(&index, &node.id, false, None, depth, limit)).collect::<Vec<_>>(),
        "quality": quality(graph)
    })
}

pub fn guide_with_root_ids(
    graph: &CodeGraph,
    query: Option<&str>,
    root_ids: &[String],
    depth: usize,
    limit: usize,
    root_source: &str,
) -> Value {
    let index = AiIndex::new(graph);
    let roots = root_ids
        .iter()
        .filter_map(|id| index.node(id))
        .take(limit.max(1))
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return guide(graph, query, depth, limit);
    }
    json!({
        "kind": "guide",
        "query": query,
        "root_source": root_source,
        "read_order": read_order(&index, &roots, limit),
        "roots": roots.iter().map(|node| node_value(&index, node)).collect::<Vec<_>>(),
        "callers": roots.iter().flat_map(|node| walk(&index, &node.id, false, Some(EdgeKind::Calls), depth, limit)).collect::<Vec<_>>(),
        "callees": roots.iter().flat_map(|node| walk(&index, &node.id, true, Some(EdgeKind::Calls), depth, limit)).collect::<Vec<_>>(),
        "impact": roots.iter().flat_map(|node| walk(&index, &node.id, false, None, depth, limit)).collect::<Vec<_>>(),
        "quality": quality(graph)
    })
}

struct Cluster<'a> {
    name: String,
    files: BTreeSet<String>,
    nodes: usize,
    symbols: usize,
    degree: usize,
    hot: Vec<(usize, &'a Node)>,
}

fn entry_candidates(graph: &CodeGraph, index: &AiIndex, limit: usize) -> Vec<Value> {
    entry_candidates_raw(graph, index, limit)
        .into_iter()
        .map(|(score, node, reasons)| {
            json!({
                "score": score,
                "reasons": reasons,
                "node": node_value(index, node)
            })
        })
        .collect()
}

fn entry_candidates_raw<'a>(
    graph: &'a CodeGraph,
    index: &AiIndex<'a>,
    limit: usize,
) -> Vec<(usize, &'a Node, Vec<String>)> {
    let target_files = graph
        .project
        .packages
        .iter()
        .flat_map(|package| &package.targets)
        .map(|target| target.src_path.as_str())
        .collect::<Vec<_>>();
    let mut items =
        graph
            .nodes
            .iter()
            .filter(|node| !matches!(node.kind, NodeKind::Project | NodeKind::Crate))
            .filter_map(|node| {
                let mut score = index.degree(&node.id);
                let mut reasons = Vec::new();
                if node.name == "main" {
                    score += 180;
                    reasons.push("main function".to_string());
                }
                if node.name.starts_with("run_")
                    || node.name.ends_with("_handler")
                    || node.name.ends_with("_route")
                {
                    score += 75;
                    reasons.push("workflow-like name".to_string());
                }
                if node.visibility.as_deref() == Some("pub") {
                    score += 35;
                    reasons.push("public API".to_string());
                }
                if node.file.as_deref().is_some_and(|file| {
                    file.ends_with("src/main.rs") || file.ends_with("src/lib.rs")
                }) {
                    score += 55;
                    reasons.push("crate boundary file".to_string());
                }
                if node.file.as_deref().is_some_and(|file| {
                    target_files.iter().any(|target| {
                        target.ends_with(file)
                            || file.ends_with("src/main.rs")
                            || file.ends_with("src/lib.rs")
                    })
                }) {
                    score += 45;
                    reasons.push("cargo target".to_string());
                }
                if matches!(
                    node.kind,
                    NodeKind::Function
                        | NodeKind::Method
                        | NodeKind::Trait
                        | NodeKind::Struct
                        | NodeKind::Enum
                ) {
                    score += 12;
                }
                if reasons.is_empty() && score < 10 {
                    return None;
                }
                Some((score, node, reasons))
            })
            .collect::<Vec<_>>();
    items.sort_by_key(|(score, node, _)| {
        (
            std::cmp::Reverse(*score),
            node.file.clone().unwrap_or_default(),
            node.qualified_name.clone(),
        )
    });
    items.into_iter().take(limit).collect()
}

fn read_order(index: &AiIndex, roots: &[&Node], limit: usize) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    for root in roots {
        push_read_item(index, &mut seen, &mut items, root, "root");
        for item in index
            .edges(&root.id, true)
            .iter()
            .filter(|edge| {
                matches!(
                    edge.kind,
                    EdgeKind::Declares | EdgeKind::ModuleFile | EdgeKind::Calls
                )
            })
            .filter_map(|edge| index.node(&edge.to))
            .filter(|node| is_readable_node(node))
            .take(limit)
        {
            push_read_item(index, &mut seen, &mut items, item, "direct dependency");
        }
        if items.len() >= limit {
            break;
        }
    }
    items.into_iter().take(limit).collect()
}

fn push_read_item(
    index: &AiIndex,
    seen: &mut BTreeSet<String>,
    items: &mut Vec<Value>,
    node: &Node,
    reason: &str,
) {
    let Some(file) = node.file.as_deref() else {
        return;
    };
    if !is_readable_node(node) {
        return;
    }
    let key = format!(
        "{}:{}",
        file,
        node.range
            .as_ref()
            .map(|range| range.start_line)
            .unwrap_or_default()
    );
    if !seen.insert(key) {
        return;
    }
    items.push(json!({
        "reason": reason,
        "file": file,
        "range": node.range,
        "node": node_value(index, node)
    }));
}

fn walk(
    index: &AiIndex,
    start: &str,
    outbound: bool,
    kind: Option<EdgeKind>,
    depth: usize,
    limit: usize,
) -> Vec<Value> {
    let mut seen = BTreeSet::from([start.to_string()]);
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
            .filter(|edge| kind.as_ref().is_none_or(|kind| edge.kind == *kind))
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

fn search_nodes<'a>(
    graph: &'a CodeGraph,
    index: &AiIndex,
    query: &str,
    limit: usize,
) -> Vec<&'a Node> {
    let terms = query
        .to_lowercase()
        .split(|char: char| !char.is_ascii_alphanumeric() && char != '_')
        .filter(|term| !term.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut items = graph
        .nodes
        .iter()
        .filter_map(|node| {
            let haystack = format!(
                "{} {} {} {}",
                node.name,
                node.qualified_name,
                node.file.as_deref().unwrap_or_default(),
                node.docs.as_deref().unwrap_or_default()
            )
            .to_lowercase();
            let text_score = terms
                .iter()
                .map(|term| {
                    if haystack == *term {
                        100
                    } else if haystack.contains(term) {
                        25
                    } else {
                        0
                    }
                })
                .sum::<usize>();
            (text_score > 0).then_some((text_score + index.degree(&node.id), node))
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|(score, node)| (std::cmp::Reverse(*score), node.qualified_name.clone()));
    items
        .into_iter()
        .take(limit)
        .map(|(_, node)| node)
        .collect()
}

fn cluster_key(file: &str) -> String {
    let mut parts = file.split('/').collect::<Vec<_>>();
    if parts.first() == Some(&"src") {
        parts.remove(0);
    }
    match parts.as_slice() {
        [] => "root".to_string(),
        [single] => single.trim_end_matches(".rs").to_string(),
        [first, second, ..] if *first == "bin" => format!("bin/{second}"),
        [first, second, ..] => format!("{first}/{second}"),
    }
}

fn is_readable_node(node: &Node) -> bool {
    matches!(
        node.kind,
        NodeKind::File
            | NodeKind::Module
            | NodeKind::Function
            | NodeKind::Method
            | NodeKind::Struct
            | NodeKind::Enum
            | NodeKind::Trait
            | NodeKind::Impl
            | NodeKind::TypeAlias
            | NodeKind::Const
            | NodeKind::Static
            | NodeKind::Macro
    )
}

fn quality_label(score: usize) -> &'static str {
    match score {
        85..=100 => "high",
        65..=84 => "medium",
        1..=64 => "low",
        _ => "empty",
    }
}

fn quality_recommendations(
    graph: &CodeGraph,
    score: usize,
    semantic_enabled: bool,
    mir_enabled: bool,
    possible: usize,
) -> Vec<String> {
    let mut items = Vec::new();
    if !semantic_enabled {
        items.push(
            "Run `index --semantic` to confirm more call edges with rust-analyzer.".to_string(),
        );
    }
    if !mir_enabled {
        items.push("Run `index --mir` when lowered call confirmation matters.".to_string());
    }
    if possible > graph.edges.len().saturating_div(10).max(1) {
        items.push(
            "Many edges are possible dispatch candidates; treat trait-call paths as hypotheses."
                .to_string(),
        );
    }
    if !graph.warnings.is_empty() {
        items.push("Inspect warnings before trusting missing or sparse regions.".to_string());
    }
    if score < 65 {
        items.push(
            "Use source reads for final confirmation; this graph has a low confidence score."
                .to_string(),
        );
    }
    items
}

fn node_value(index: &AiIndex, node: &Node) -> Value {
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

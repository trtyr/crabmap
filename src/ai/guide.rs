use crate::model::{CodeGraph, EdgeKind, Node, NodeKind};
use serde_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};

use super::entries::entry_candidates_raw;
use super::index::AiIndex;
use super::quality::quality;

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

pub(super) fn node_value(index: &AiIndex, node: &Node) -> Value {
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

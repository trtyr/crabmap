use crate::model::{CodeGraph, Node};
use anyhow::Result;
use petgraph::algo::astar;
use petgraph::graphmap::DiGraphMap;
use serde_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};

use super::find::require_unique_node;
use super::index::QueryIndex;

pub(crate) fn node_value(index: &QueryIndex, node: &Node) -> Value {
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

pub(crate) fn adjacent(
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

pub(crate) fn walk(
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

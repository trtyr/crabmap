use crate::model::{CodeGraph, EdgeKind, NodeKind};
use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::find::{find_nodes, require_unique_node, suggest};
use super::index::QueryIndex;
use super::ranking::{hot_symbols, ranked_nodes};
use super::traversal::{adjacent, node_value, walk};

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
            let files: Vec<&str> = graph
                .nodes
                .iter()
                .filter(|n| n.kind == NodeKind::File)
                .map(|n| n.name.as_str())
                .collect();
            format!("file `{path}` not found{}", suggest(path, &files, 3))
        })?;
    let declares = index
        .edges(&node.id, true)
        .iter()
        .find(|edge| edge.kind == EdgeKind::ModuleFile)
        .and_then(|edge| {
            index
                .node(&edge.to)
                .map(|module| adjacent(&index, &module.id, true, Some("declares"), 500))
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
            .find(|candidate| {
                !adjacent(&index, &candidate.id, true, Some("declares"), 1).is_empty()
            })
            .or_else(|| matches.first().copied())
    }
    .with_context(|| {
        let mods: Vec<&str> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Module)
            .map(|n| n.name.as_str())
            .collect();
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

use crate::model::{CodeGraph, EdgeKind, NodeKind};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use super::filter::SymbolFilter;
use super::find::{find_nodes, require_unique_node, suggest};
use super::index::QueryIndex;
use super::ranking::{hot_symbols, ranked_nodes};
use super::similar::similar;
use super::traversal::{adjacent, node_value, walk};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum TraceDirection {
    Up,
    Down,
    Both,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum FindMode {
    Text,
    Similar,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum ScopeKind {
    File,
    Module,
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

pub fn symbols(graph: &CodeGraph, query: Option<&str>, kind: Option<&str>, limit: usize, filter: SymbolFilter) -> Value {
    let index = QueryIndex::new(graph);
    let filtered: Vec<_> = ranked_nodes(graph, &index, query.unwrap_or(""), limit)
        .into_iter()
        .filter(|node| kind.is_none_or(|kind| node.kind.as_str() == kind))
        .filter(|node| filter.matches(node, &index))
        .map(|node| node_value(&index, node))
        .collect();
    json!({
        "kind": "symbols",
        "query": query,
        "kind_filter": kind,
        "applied_filters": filter.description(),
        "count": filtered.len(),
        "items": filtered
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
    let callers = walk(&index, &node.id, false, Some("calls"), depth, limit);
    let dependents = walk(&index, &node.id, false, None, depth, limit);
    let dependencies = walk(&index, &node.id, true, None, depth, limit);

    // 1. files_affected: group all callers by file
    let mut file_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in &callers {
        if let Some(node_val) = item.get("node") {
            let file = node_val
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>")
                .to_string();
            let name = node_val
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            file_groups.entry(file).or_default().push(name);
        }
    }
    let mut files_affected: Vec<Value> = file_groups
        .into_iter()
        .map(|(file, mut names)| {
            names.sort();
            names.dedup();
            let count = names.len();
            json!({
                "file": file,
                "nodes": count,
                "examples": names.into_iter().take(5).collect::<Vec<_>>()
            })
        })
        .collect();
    files_affected.sort_by_key(|item| {
        std::cmp::Reverse(item.get("nodes").and_then(Value::as_u64).unwrap_or_default())
    });

    // 2. call_sites: direct callers (depth=1) with call_style
    let mut call_sites: Vec<Value> = Vec::new();
    for edge in index.edges(&node.id, false) {
        if edge.kind != EdgeKind::Calls {
            continue;
        }
        if let Some(caller_node) = index.node(&edge.from) {
            let location = edge.evidence.as_ref();
            let file_str = location
                .map(|loc| format!("{}:{}", loc.file, loc.line))
                .or_else(|| {
                    caller_node.file.as_ref().map(|f| {
                        let line = caller_node
                            .range
                            .as_ref()
                            .map(|r| r.start_line)
                            .unwrap_or(0);
                        format!("{}:{}", f, line)
                    })
                })
                .unwrap_or_default();
            let at_line = location.map(|loc| loc.line).unwrap_or_else(|| {
                caller_node
                    .range
                    .as_ref()
                    .map(|r| r.start_line)
                    .unwrap_or(0)
            });
            call_sites.push(json!({
                "caller": caller_node.name,
                "file": file_str,
                "call_style": edge.call_style.as_deref().unwrap_or("direct"),
                "at_line": at_line
            }));
        }
    }

    // 3. change_hints: heuristic analysis
    let mut change_hints: Vec<String> = Vec::new();
    let unique_files: BTreeSet<&str> = callers
        .iter()
        .filter_map(|item| item.get("file").or_else(|| item.get("node")?.get("file")))
        .filter_map(Value::as_str)
        .collect();
    let file_count = unique_files.len();
    if file_count == 0 {
        change_hints.push("Safe to remove — no callers found".to_string());
    } else if file_count >= 3 {
        change_hints.push(format!(
            "High: changes propagate to {} files",
            file_count
        ));
    } else {
        change_hints.push(format!(
            "Low: contained change ({} file{})",
            file_count,
            if file_count == 1 { "" } else { "s" }
        ));
    }
    if node.visibility.as_deref() == Some("pub") && !callers.is_empty() {
        change_hints.push("Consider deprecation period — symbol is pub and has callers".to_string());
    }
    let has_method_caller = call_sites.iter().any(|cs| {
        cs.get("call_style")
            .and_then(Value::as_str)
            .is_some_and(|s| s == "method")
    });
    if has_method_caller {
        change_hints.push("Check trait impls for breaking changes — method call style detected".to_string());
    }

    Ok(json!({
        "kind": "impact",
        "root": node_value(&index, node),
        "callers": callers,
        "dependents": dependents,
        "dependencies": dependencies,
        "files_affected": files_affected,
        "call_sites": call_sites,
        "change_hints": change_hints
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

pub fn inspect(graph: &CodeGraph, name: &str, include_source: bool) -> Result<Value> {
    let index = QueryIndex::new(graph);
    let matches = find_nodes(graph, name);
    if matches.is_empty() {
        let names: Vec<&str> = graph.nodes.iter().map(|n| n.name.as_str()).collect();
        anyhow::bail!("symbol `{name}` not found{}", suggest(name, &names, 3));
    }
    if matches.len() > 1 {
        return Ok(json!({
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
        }));
    }

    let node = matches[0];
    let mut value = json!({
        "kind": "inspect",
        "node": node_value(&index, node),
        "incoming": adjacent(&index, &node.id, false, None, 100),
        "outgoing": adjacent(&index, &node.id, true, None, 100)
    });

    if include_source {
        let file = node
            .file
            .as_deref()
            .with_context(|| format!("symbol `{name}` has no file"))?;
        let range = node
            .range
            .clone()
            .with_context(|| format!("symbol `{name}` has no range"))?;
        let content = fs::read_to_string(file)
            .with_context(|| format!("failed to read source file `{file}`"))?;
        let lines: Vec<&str> = content.lines().collect();
        let start = range.start_line.saturating_sub(1);
        let end = range.end_line.min(lines.len());
        let snippet: Vec<String> = lines[start..end].iter().map(|s| s.to_string()).collect();
        value["source"] = json!({
            "content": snippet.join("\n"),
            "range": range,
            "line_count": snippet.len()
        });
    }

    Ok(value)
}

pub fn trace(
    graph: &CodeGraph,
    name: &str,
    direction: TraceDirection,
    depth: usize,
    limit: usize,
) -> Result<Value> {
    let index = QueryIndex::new(graph);
    let node = require_unique_node(graph, name, "symbol")?;

    let value = match direction {
        TraceDirection::Up => json!({
            "kind": "trace",
            "direction": "up",
            "root": node_value(&index, node),
            "depth": depth,
            "items": neighbors(graph, name, "calls", false, depth, limit)?["items"].clone()
        }),
        TraceDirection::Down => json!({
            "kind": "trace",
            "direction": "down",
            "root": node_value(&index, node),
            "depth": depth,
            "items": neighbors(graph, name, "calls", true, depth, limit)?["items"].clone()
        }),
        TraceDirection::Both => json!({
            "kind": "trace",
            "direction": "both",
            "root": node_value(&index, node),
            "depth": depth,
            "upstream": neighbors(graph, name, "calls", false, depth, limit)?["items"].clone(),
            "downstream": neighbors(graph, name, "calls", true, depth, limit)?["items"].clone()
        }),
    };

    Ok(value)
}

pub fn find(graph: &CodeGraph, pattern: &str, mode: FindMode, limit: usize) -> Value {
    match mode {
        FindMode::Text => {
            let index = QueryIndex::new(graph);
            json!({
                "kind": "find",
                "mode": "text",
                "pattern": pattern,
                "items": ranked_nodes(graph, &index, pattern, limit)
                    .into_iter()
                    .map(|node| node_value(&index, node))
                    .collect::<Vec<_>>()
            })
        }
        FindMode::Similar => {
            let mut value = similar(graph, pattern, limit);
            if value.get("kind").and_then(Value::as_str) != Some("ambiguous") {
                value["kind"] = json!("find");
                value["mode"] = json!("similar");
                value["pattern"] = json!(pattern);
            }
            value
        }
    }
}

pub fn scope(graph: &CodeGraph, target: &str, kind: ScopeKind) -> Result<Value> {
    let mut value = match kind {
        ScopeKind::File => file(graph, target)?,
        ScopeKind::Module => module(graph, target)?,
    };
    value["kind"] = json!("scope");
    Ok(value)
}

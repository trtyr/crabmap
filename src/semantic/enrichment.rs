use crate::model::{
    CodeGraph, Edge, EdgeCertainty, EdgeKind, EdgeSource, Location, Node, NodeKind,
    SemanticInfo,
};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

use super::helpers::{
    detect_rust_analyzer, flatten_document_symbols, is_transient_lsp_error, lsp_kind, lsp_location,
    lsp_range, path_uri, relative, text_document, unique_id, uri_path, workspace_root,
};
use super::lsp_client::LspClient;

pub fn enrich(graph: &mut CodeGraph, project: &Path, limit: usize) -> Result<()> {
    let root = workspace_root(graph, project)?;
    let Some(command) = detect_rust_analyzer() else {
        graph
            .warnings
            .push("semantic enrichment skipped: rust-analyzer not found in PATH".to_string());
        graph.semantic = Some(SemanticInfo {
            provider: "rust-analyzer".to_string(),
            enabled: false,
            scanned_symbols: 0,
            enriched_symbols: 0,
            confirmed_symbols: 0,
            enriched_edges: 0,
            confirmed_edges: 0,
            unresolved_items: 0,
            warnings: vec!["rust-analyzer not found in PATH".to_string()],
        });
        return Ok(());
    };
    let mut client = LspClient::start(&command, &root)?;
    client.initialize(&root)?;
    std::thread::sleep(std::time::Duration::from_millis(750));

    let mut scanned = 0;
    let mut enriched_symbols = 0;
    let mut confirmed_symbols = 0;
    let mut enriched = 0;
    let mut confirmed = 0;
    let mut unresolved = 0;
    let mut warnings = Vec::new();
    for file in graph_files(graph) {
        match enrich_document_symbols(graph, &mut client, &root, &file) {
            Ok(stats) => {
                enriched_symbols += stats.0;
                confirmed_symbols += stats.1;
            }
            Err(error) => warnings.push(format!("{file}: {error:#}")),
        }
    }
    let symbols = graph
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method))
        .filter(|node| node.file.is_some() && node.range.is_some())
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();

    for node in symbols {
        scanned += 1;
        match enrich_symbol(graph, &mut client, &root, &node) {
            Ok(stats) => {
                enriched += stats.0;
                confirmed += stats.1;
                unresolved += stats.2;
            }
            Err(error) if !is_transient_lsp_error(&error) => {
                warnings.push(format!("{}: {error:#}", node.qualified_name))
            }
            Err(_) => {}
        }
    }
    client.shutdown();
    graph.semantic = Some(SemanticInfo {
        provider: "rust-analyzer".to_string(),
        enabled: true,
        scanned_symbols: scanned,
        enriched_symbols,
        confirmed_symbols,
        enriched_edges: enriched,
        confirmed_edges: confirmed,
        unresolved_items: unresolved,
        warnings,
    });
    Ok(())
}

fn graph_files(graph: &CodeGraph) -> Vec<String> {
    graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .map(|node| node.name.clone())
        .collect()
}

fn enrich_document_symbols(
    graph: &mut CodeGraph,
    client: &mut LspClient,
    root: &Path,
    file: &str,
) -> Result<(usize, usize)> {
    let path = root.join(file);
    client.did_open(&path)?;
    let mut result = Value::Null;
    for attempt in 0..3 {
        result = client.request(
            "textDocument/documentSymbol",
            json!({ "textDocument": text_document(&path)? }),
        )?;
        if result.as_array().is_some_and(|items| !items.is_empty()) || attempt == 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let mut enriched = 0;
    let mut confirmed = 0;
    let uri = path_uri(&path)?;
    for mut item in flatten_document_symbols(&result) {
        if let Some(item) = item.as_object_mut() {
            item.entry("uri").or_insert(Value::String(uri.clone()));
        }
        if confirm_or_insert_symbol(graph, root, &item) {
            confirmed += 1;
            continue;
        }
        if resolve_or_insert(graph, root, &item).is_some() {
            enriched += 1;
        }
    }
    Ok((enriched, confirmed))
}

fn confirm_or_insert_symbol(graph: &mut CodeGraph, root: &Path, item: &Value) -> bool {
    let Some(name) = item.get("name").and_then(Value::as_str) else {
        return false;
    };
    let file = item
        .get("uri")
        .and_then(Value::as_str)
        .and_then(uri_path)
        .map(|path| relative(root, &path));
    let range = item.get("selectionRange").or_else(|| item.get("range"));
    let line = range
        .and_then(lsp_range)
        .map(|range| range.start_line)
        .unwrap_or_default();
    if let Some(node) = graph.nodes.iter_mut().find(|node| {
        node.name == name
            && file
                .as_ref()
                .is_none_or(|file| node.file.as_ref() == Some(file))
            && node
                .range
                .as_ref()
                .is_none_or(|range| line == 0 || range.start_line == line)
    }) {
        node.metrics
            .insert("rust_analyzer_document_symbol".to_string(), 1);
        return true;
    }
    let candidates = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.name == name
                && file
                    .as_ref()
                    .is_none_or(|file| node.file.as_ref() == Some(file))
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return false;
    }
    if let Some(node) = graph.nodes.get_mut(candidates[0]) {
        node.metrics
            .insert("rust_analyzer_document_symbol".to_string(), 1);
        return true;
    }
    false
}

fn enrich_symbol(
    graph: &mut CodeGraph,
    client: &mut LspClient,
    root: &Path,
    node: &Node,
) -> Result<(usize, usize, usize)> {
    let file = root.join(node.file.as_ref().context("semantic node missing file")?);
    if !file.exists() {
        bail!("source file not found: {}", file.display());
    }
    client.did_open(&file)?;
    let position = symbol_position(&file, node)?;
    let items = client.request(
        "textDocument/prepareCallHierarchy",
        json!({
            "textDocument": text_document(&file)?,
            "position": position
        }),
    )?;
    let Some(item) = items.as_array().and_then(|items| items.first()).cloned() else {
        return Ok((0, 0, 0));
    };
    let outgoing = client.request("callHierarchy/outgoingCalls", json!({ "item": item }))?;
    let mut enriched = 0;
    let mut confirmed = 0;
    let mut unresolved = 0;
    for call in outgoing.as_array().into_iter().flatten() {
        let Some(target) = call.get("to") else {
            unresolved += 1;
            continue;
        };
        let target_id = resolve_or_insert(graph, root, target);
        if let Some(target_id) = target_id {
            match add_edge(
                graph,
                &node.id,
                &target_id,
                EdgeKind::Calls,
                Some("rust-analyzer outgoing call".to_string()),
                lsp_location(root, target),
            ) {
                EdgeResult::Inserted => enriched += 1,
                EdgeResult::Confirmed => confirmed += 1,
                EdgeResult::Skipped => {}
            }
            continue;
        }
        unresolved += 1;
    }
    Ok((enriched, confirmed, unresolved))
}

fn resolve_or_insert(graph: &mut CodeGraph, root: &Path, item: &Value) -> Option<String> {
    let name = item.get("name").and_then(Value::as_str)?.to_string();
    let absolute_file = item.get("uri").and_then(Value::as_str).and_then(uri_path);
    if absolute_file
        .as_ref()
        .is_some_and(|path| !path.starts_with(root))
    {
        return None;
    }
    let file = absolute_file.map(|path| relative(root, &path));
    if let Some(existing) = graph.nodes.iter().find(|node| {
        node.name == name
            && file
                .as_ref()
                .is_none_or(|file| node.file.as_ref() == Some(file))
    }) {
        return Some(existing.id.clone());
    }
    let qualified_name = item
        .get("detail")
        .and_then(Value::as_str)
        .filter(|detail| !detail.trim().is_empty())
        .map(|detail| format!("{detail}::{name}"))
        .unwrap_or_else(|| {
            file.as_ref()
                .map(|file| format!("{file}::{name}"))
                .unwrap_or_else(|| name.clone())
        });
    let id = unique_id(
        graph,
        format!("{}:{qualified_name}", lsp_kind(item).as_str()).replace([' ', '\n', '\t'], "_"),
    );
    graph.nodes.push(Node {
        id: id.clone(),
        kind: lsp_kind(item),
        name,
        qualified_name,
        file,
        range: item.get("range").and_then(lsp_range),
        visibility: None,
        signature: item
            .get("detail")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        docs: None,
        metrics: BTreeMap::new(),
    });
    Some(id)
}

fn add_edge(
    graph: &mut CodeGraph,
    from: &str,
    to: &str,
    kind: EdgeKind,
    label: Option<String>,
    evidence: Option<Location>,
) -> EdgeResult {
    if from == to {
        return EdgeResult::Skipped;
    }
    if let Some(edge) = graph
        .edges
        .iter_mut()
        .find(|edge| edge.from == from && edge.to == to && edge.kind == kind)
    {
        edge.weight += 1;
        edge.source = EdgeSource::RustAnalyzer;
        edge.certainty = EdgeCertainty::Confirmed;
        return EdgeResult::Confirmed;
    }
    let profiles = graph
        .profiles
        .iter()
        .map(|profile| profile.name.clone())
        .collect();
    graph.edges.push(Edge {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        label,
        evidence,
        weight: 1,
        source: EdgeSource::RustAnalyzer,
        certainty: EdgeCertainty::Confirmed,
        call_style: None,
        profiles,
    });
    EdgeResult::Inserted
}

enum EdgeResult {
    Inserted,
    Confirmed,
    Skipped,
}

fn symbol_position(file: &Path, node: &Node) -> Result<Value> {
    let source = std::fs::read_to_string(file)?;
    let line = node
        .range
        .as_ref()
        .map(|range| range.start_line)
        .unwrap_or(1)
        .saturating_sub(1);
    let character = source
        .lines()
        .nth(line)
        .and_then(|text| text.find(&node.name))
        .unwrap_or_default();
    Ok(json!({ "line": line, "character": character }))
}

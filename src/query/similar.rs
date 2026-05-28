use crate::model::{CodeGraph, EdgeKind, Node, NodeKind};
use serde_json::{Value, json};
use std::collections::HashSet;

use super::find::find_nodes;
use super::index::QueryIndex;
use super::traversal::node_value;

pub fn similar(graph: &CodeGraph, name: &str, limit: usize) -> Value {
    let index = QueryIndex::new(graph);
    let matches = find_nodes(graph, name);
    if matches.is_empty() {
        return json!({"kind": "similar", "name": name, "items": []});
    }
    if matches.len() > 1 {
        return json!({
            "kind": "ambiguous",
            "name": name,
            "matches": matches.iter().map(|node| json!({
                "id": node.id, "name": node.name,
                "qualified_name": node.qualified_name,
                "kind": node.kind.as_str(),
                "file": node.file, "range": node.range
            })).collect::<Vec<_>>()
        });
    }
    let target = matches[0];
    let target_kind = target.kind.clone();
    let target_callees: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|e| e.from == target.id && matches!(e.kind, EdgeKind::Calls))
        .map(|e| e.to.as_str())
        .collect();
    let mut items: Vec<(usize, &Node)> = graph
        .nodes
        .iter()
        .filter(|node| node.kind.as_str() == target_kind.as_str() && node.id != target.id)
        .filter(|node| {
            !matches!(
                node.kind,
                NodeKind::Project | NodeKind::Crate | NodeKind::File | NodeKind::Module
            )
        })
        .map(|node| {
            let node_callees: HashSet<&str> = graph
                .edges
                .iter()
                .filter(|e| e.from == node.id && matches!(e.kind, EdgeKind::Calls))
                .map(|e| e.to.as_str())
                .collect();
            let overlap = target_callees.intersection(&node_callees).count();
            let sig_sim = signature_similarity(
                target.signature.as_deref().unwrap_or(""),
                node.signature.as_deref().unwrap_or(""),
            );
            let score = overlap * 3 + sig_sim;
            (score, node)
        })
        .filter(|(score, _)| *score > 0)
        .collect();
    items.sort_by_key(|(score, node)| (std::cmp::Reverse(*score), node.qualified_name.clone()));
    let count = items.len();
    json!({
        "kind": "similar",
        "name": target.qualified_name,
        "kind_match": target_kind.as_str(),
        "count": count,
        "items": items.into_iter().take(limit).map(|(score, node)| {
            let mut value = node_value(&index, node);
            value["similarity_score"] = json!(score);
            value
        }).collect::<Vec<_>>()
    })
}

fn signature_similarity(a: &str, b: &str) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let mut score = 0;
    for kw in &[
        "fn",
        "pub",
        "async",
        "unsafe",
        "Result",
        "Option",
        "Vec",
        "HashMap",
        "String",
        "u32",
        "u64",
        "i32",
        "i64",
        "bool",
        "&str",
        "self",
        "&self",
        "&mut self",
        "PathBuf",
        "io::Result",
    ] {
        if a.contains(kw) == b.contains(kw) {
            score += 1;
        }
    }
    score
}

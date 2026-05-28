use crate::model::{CodeGraph, Node, NodeKind};
use serde_json::{Value, json};

use super::guide::node_value;
use super::index::AiIndex;

pub fn entries(graph: &CodeGraph, limit: usize) -> Value {
    let index = AiIndex::new(graph);
    json!({
        "kind": "entries",
        "items": entry_candidates(graph, &index, limit)
    })
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

pub(super) fn entry_candidates_raw<'a>(
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

use crate::model::{CodeGraph, Node, NodeKind};
use serde_json::{Value, json};

use super::index::QueryIndex;
use super::traversal::node_value;

pub(crate) fn ranked_nodes<'a>(
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

pub(crate) fn hot_symbols(graph: &CodeGraph, index: &QueryIndex, limit: usize) -> Vec<Value> {
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

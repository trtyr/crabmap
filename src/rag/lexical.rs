use crate::model::{CodeGraph, Node, NodeKind};

use super::helpers::{document_text, is_retrievable, sort_docs};
use super::types::Document;

pub(super) fn lexical_candidates<'a>(
    graph: &'a CodeGraph,
    query: &str,
    limit: usize,
) -> Vec<Document<'a>> {
    let terms = terms(query);
    let mut docs = graph
        .nodes
        .iter()
        .filter(|node| is_retrievable(node))
        .filter_map(|node| {
            let text = document_text(node);
            let score = lexical_score(&text, &terms) + structural_score(node);
            (score > 0.0).then_some(Document {
                node,
                text,
                lexical: score,
                embedding: None,
                rerank: None,
            })
        })
        .collect::<Vec<_>>();
    sort_docs(&mut docs);
    docs.truncate(limit);
    docs
}

pub(super) fn lexical_score(text: &str, terms: &[String]) -> f64 {
    let haystack = text.to_lowercase();
    terms
        .iter()
        .map(|term| if haystack.contains(term) { 25.0 } else { 0.0 })
        .sum()
}

pub(super) fn structural_score(node: &Node) -> f64 {
    match node.kind {
        NodeKind::Function | NodeKind::Method => 9.0,
        NodeKind::Struct | NodeKind::Enum | NodeKind::Trait | NodeKind::Impl => 7.0,
        NodeKind::Module | NodeKind::File => 5.0,
        _ => 1.0,
    }
}

pub(super) fn terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|char: char| !char.is_ascii_alphanumeric() && char != '_')
        .filter(|term| !term.is_empty())
        .map(ToString::to_string)
        .collect()
}

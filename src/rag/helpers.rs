use crate::model::{Node, NodeKind};
use serde_json::{Value, json};
use std::cmp::Ordering;

use super::types::Document;

pub(super) fn sort_docs(docs: &mut [Document]) {
    docs.sort_by(|a, b| {
        score(b)
            .partial_cmp(&score(a))
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.node.qualified_name.cmp(&b.node.qualified_name))
    });
}

pub(super) fn score(doc: &Document) -> f64 {
    doc.rerank
        .map(|score| score * 1000.0)
        .or_else(|| doc.embedding.map(|score| score * 100.0 + doc.lexical))
        .unwrap_or(doc.lexical)
}

pub(super) fn document_value(doc: Document) -> Value {
    json!({
        "score": score(&doc),
        "lexical_score": doc.lexical,
        "embedding_score": doc.embedding,
        "rerank_score": doc.rerank,
        "node": {
            "id": doc.node.id,
            "kind": doc.node.kind.as_str(),
            "name": doc.node.name,
            "qualified_name": doc.node.qualified_name,
            "file": doc.node.file,
            "range": doc.node.range,
            "visibility": doc.node.visibility,
            "degree": doc.node.metrics.get("degree")
        },
        "text": doc.text
    })
}

pub(super) fn document_text(node: &Node) -> String {
    [
        format!("kind: {}", node.kind.as_str()),
        format!("name: {}", node.name),
        format!("qualified_name: {}", node.qualified_name),
        format!("file: {}", node.file.as_deref().unwrap_or_default()),
        format!(
            "range: {}",
            node.range
                .as_ref()
                .map(|range| format!("{}-{}", range.start_line, range.end_line))
                .unwrap_or_default()
        ),
        format!(
            "signature: {}",
            truncate(node.signature.as_deref().unwrap_or_default(), 420)
        ),
        format!(
            "docs: {}",
            truncate(node.docs.as_deref().unwrap_or_default(), 420)
        ),
    ]
    .join("\n")
}

pub(super) fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let dot = a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
    let norm_a = a.iter().map(|value| value * value).sum::<f64>().sqrt();
    let norm_b = b.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

pub(super) fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }
    format!("{}...", &value[..limit])
}

pub(super) fn is_retrievable(node: &Node) -> bool {
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

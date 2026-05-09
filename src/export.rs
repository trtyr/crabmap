use crate::cli::ExportFormat;
use crate::model::CodeGraph;
use anyhow::Result;
use serde_json::{Value, json};

pub fn export(graph: &CodeGraph, format: ExportFormat) -> Result<Value> {
    match format {
        ExportFormat::Json => Ok(serde_json::to_value(graph)?),
        ExportFormat::Dot => Ok(json!({ "kind": "dot", "content": dot(graph) })),
        ExportFormat::Mermaid => Ok(json!({ "kind": "mermaid", "content": mermaid(graph) })),
    }
}

fn dot(graph: &CodeGraph) -> String {
    let mut lines = vec![
        "digraph codegraph {".to_string(),
        "  rankdir=LR;".to_string(),
    ];
    for node in &graph.nodes {
        lines.push(format!(
            "  \"{}\" [label=\"{}\\n{}\"]",
            escape(&node.id),
            escape(&node.name),
            node.kind.as_str()
        ));
    }
    for edge in &graph.edges {
        lines.push(format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"]",
            escape(&edge.from),
            escape(&edge.to),
            edge.kind.as_str()
        ));
    }
    lines.push("}".to_string());
    lines.join("\n")
}

fn mermaid(graph: &CodeGraph) -> String {
    let mut lines = vec!["graph LR".to_string()];
    for edge in &graph.edges {
        lines.push(format!(
            "  {}[\"{}\"] -->|{}| {}[\"{}\"]",
            id(&edge.from),
            label(graph, &edge.from),
            edge.kind.as_str(),
            id(&edge.to),
            label(graph, &edge.to)
        ));
    }
    lines.join("\n")
}

fn label(graph: &CodeGraph, id: &str) -> String {
    escape(
        graph
            .nodes
            .iter()
            .find(|node| node.id == id)
            .map(|node| node.name.as_str())
            .unwrap_or(id),
    )
}

fn id(value: &str) -> String {
    value
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() {
                char
            } else {
                '_'
            }
        })
        .collect()
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

use crate::ai;
use crate::health;
use crate::model::CodeGraph;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn write(graph: &CodeGraph, output: Option<&Path>) -> Result<Value> {
    let dir = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(&graph.project.root).join(".codegraph"));
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let graph_report = dir.join("GRAPH_REPORT.md");
    let agent_guide = dir.join("AGENT_GUIDE.md");
    std::fs::write(&graph_report, graph_report_markdown(graph)?)
        .with_context(|| format!("failed to write {}", graph_report.display()))?;
    std::fs::write(&agent_guide, agent_guide_markdown())
        .with_context(|| format!("failed to write {}", agent_guide.display()))?;
    Ok(serde_json::json!({
        "kind": "report",
        "graph_report": graph_report,
        "agent_guide": agent_guide
    }))
}

pub fn graph_report_markdown(graph: &CodeGraph) -> Result<String> {
    let quality = ai::quality(graph);
    let entries = ai::entries(graph, 12);
    let clusters = ai::clusters(graph, 12);
    let health = health::health(graph, 12);
    let stats = graph.stats();
    Ok(format!(
        r#"# Codegraph Report

Project: `{}`

## Snapshot

- Nodes: {}
- Edges: {}
- Files: {}
- Symbols: {}
- Quality: {} ({})
- Health: {} ({})

## AI Reading Rule

Before grepping or reading many files, use this report to choose a small set of likely files, then confirm details in source.

## Entry Points

{}

## Feature Clusters

{}

## Architecture Health

{}

## Quality Notes

{}
"#,
        graph.project.root,
        stats.nodes,
        stats.edges,
        stats.files,
        stats.symbols,
        quality
            .get("score")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        quality
            .get("interpretation")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        health
            .get("score")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        health
            .get("interpretation")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        entries_markdown(&entries),
        clusters_markdown(&clusters),
        health_markdown(&health),
        quality_markdown(&quality)
    ))
}

fn agent_guide_markdown() -> &'static str {
    r#"# Agent Guide

Use codegraph before broad source exploration.

Recommended flow:

1. Read `.codegraph/GRAPH_REPORT.md`.
2. Use `codegraph retrieve "<task>"` to find relevant symbols.
3. Use `codegraph guide "<task>"` for read order and impact.
4. Use `codegraph callers`, `codegraph callees`, or `codegraph impact` for focused follow-up.
5. Read only the source files and line ranges identified by the graph.

Do not treat possible or inferred graph edges as facts. Confirm critical behavior in source.
"#
}

fn entries_markdown(value: &Value) -> String {
    value
        .get("items")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .map(|item| {
            let node = item.get("node").unwrap_or(&Value::Null);
            format!(
                "- `{}` `{}` in `{}` score={} reasons={}",
                node.get("kind").and_then(Value::as_str).unwrap_or("node"),
                node.get("qualified_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                node.get("file").and_then(Value::as_str).unwrap_or_default(),
                item.get("score")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                item.get("reasons")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn clusters_markdown(value: &Value) -> String {
    value
        .get("items")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .map(|item| {
            format!(
                "- `{}` files={} symbols={} degree={}",
                item.get("name").and_then(Value::as_str).unwrap_or_default(),
                item.get("files")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                item.get("symbols")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                item.get("degree")
                    .and_then(Value::as_u64)
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn health_markdown(value: &Value) -> String {
    format!(
        r#"### Hot Symbols

{}

### God Modules

{}

### Cycles

{}

### Possible Dead Public Symbols

{}
"#,
        simple_list(value, "hot_symbols", "qualified_name"),
        simple_list(value, "god_modules", "file"),
        cycle_list(value),
        simple_list(value, "possible_dead_public_symbols", "qualified_name")
    )
}

fn quality_markdown(value: &Value) -> String {
    let recommendations = value
        .get("recommendations")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(Value::as_str)
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "- Trusted edges: {}\n- Uncertain edges: {}\n{}",
        value
            .get("trusted_edges")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        value
            .get("uncertain_edges")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        recommendations
    )
}

fn simple_list(value: &Value, key: &str, label: &str) -> String {
    let items = value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return "- None".to_string();
    }
    items
        .iter()
        .map(|item| {
            format!(
                "- `{}`",
                item.get(label)
                    .and_then(Value::as_str)
                    .or_else(|| item.get("name").and_then(Value::as_str))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn cycle_list(value: &Value) -> String {
    let items = value
        .get("cycles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return "- None".to_string();
    }
    items
        .iter()
        .map(|item| {
            format!(
                "- {}",
                item.get("files")
                    .and_then(Value::as_array)
                    .map(|files| files
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" -> "))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

use crate::ai;
use crate::health;
use crate::model::CodeGraph;
use anyhow::Result;
use serde_json::Value;

pub fn map(graph: &CodeGraph, budget: usize) -> Result<Value> {
    let stats = graph.stats();
    let entries = ai::entries(graph, 10);
    let clusters = ai::clusters(graph, 10);
    let health = health::health(graph, 10);
    let mut output = format!(
        "# Codegraph Map\n\nProject: `{}`\nNodes: {} Edges: {} Files: {} Symbols: {}\nQuality: {}\nHealth: {}\n\n",
        graph.project.root,
        stats.nodes,
        stats.edges,
        stats.files,
        stats.symbols,
        ai::quality(graph)
            .get("score")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        health
            .get("score")
            .and_then(Value::as_u64)
            .unwrap_or_default()
    );
    push_section(&mut output, "Entry Points", entries_list(&entries));
    push_section(&mut output, "Feature Clusters", clusters_list(&clusters));
    push_section(&mut output, "Hot Symbols", hot_list(&health));
    if output.len() > budget {
        output.truncate(budget.saturating_sub(16));
        output.push_str("\n...[truncated]\n");
    }
    Ok(serde_json::json!({
        "kind": "map",
        "budget": budget,
        "content": output
    }))
}

fn push_section(output: &mut String, title: &str, body: String) {
    output.push_str("## ");
    output.push_str(title);
    output.push_str("\n\n");
    output.push_str(&body);
    output.push_str("\n\n");
}

fn entries_list(value: &Value) -> String {
    value
        .get("items")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .map(|item| {
            let node = item.get("node").unwrap_or(&Value::Null);
            format!(
                "- {} `{}` in `{}`",
                node.get("kind").and_then(Value::as_str).unwrap_or("node"),
                node.get("qualified_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                node.get("file").and_then(Value::as_str).unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn clusters_list(value: &Value) -> String {
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

fn hot_list(value: &Value) -> String {
    value
        .get("hot_symbols")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .map(|item| {
            format!(
                "- `{}` in `{}` degree={}",
                item.get("qualified_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                item.get("file").and_then(Value::as_str).unwrap_or_default(),
                item.get("degree")
                    .and_then(Value::as_u64)
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

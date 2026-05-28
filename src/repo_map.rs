use crate::ai;
use crate::health;
use crate::model::CodeGraph;
use anyhow::Result;
use serde_json::Value;

pub fn map(graph: &CodeGraph, budget: usize, full: bool) -> Result<Value> {
    let stats = graph.stats();
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
    if full {
        let entries = ai::entries(graph, 10);
        let clusters = ai::clusters(graph, 10);
        push_section(&mut output, "Entry Points", entries_list(&entries));
        push_section(&mut output, "Feature Clusters", clusters_list(&clusters));
    }
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
            let range_str = format_range(node);
            let file = node.get("file").and_then(Value::as_str).unwrap_or_default();
            let sig = format_sig(node.get("signature").and_then(Value::as_str));
            format!(
                "- {kind} `{qname}` in `{file}{range}`{sig}",
                kind = node.get("kind").and_then(Value::as_str).unwrap_or("node"),
                qname = node
                    .get("qualified_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                file = file,
                range = range_str,
                sig = sig,
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
            let range_str = format_range(item);
            let file = item.get("file").and_then(Value::as_str).unwrap_or_default();
            let sig = format_sig(item.get("signature").and_then(Value::as_str));
            let docs = format_docs(item.get("docs").and_then(Value::as_str));
            format!(
                "- {kind} `{qname}` in `{file}{range}` degree={degree}{sig}{docs}",
                kind = item.get("kind").and_then(Value::as_str).unwrap_or("symbol"),
                qname = item
                    .get("qualified_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                file = file,
                range = range_str,
                degree = item.get("degree").and_then(Value::as_u64).unwrap_or_default(),
                sig = sig,
                docs = docs,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format range as `:start-end` or `:line` or empty string.
fn format_range(item: &Value) -> String {
    let range = match item.get("range") {
        Some(r) => r,
        None => return String::new(),
    };
    let start = range.get("start_line").and_then(Value::as_u64);
    let end = range.get("end_line").and_then(Value::as_u64);
    match (start, end) {
        (Some(s), Some(e)) if s < e => format!(":{}-{}", s, e),
        (Some(line), _) => format!(":{}", line),
        _ => String::new(),
    }
}

/// Format signature as `\n  sig: \`...\`` or empty string.
fn format_sig(sig: Option<&str>) -> String {
    match sig {
        Some(s) if !s.is_empty() => {
            let truncated = truncate(s, 120);
            format!("\n  sig: `{}`", truncated)
        }
        _ => String::new(),
    }
}

/// Format first line of docs as `\n  docs: ...` or empty string.
fn format_docs(docs: Option<&str>) -> String {
    match docs {
        Some(d) if !d.is_empty() => {
            let first_line = d.lines().next().unwrap_or("");
            if first_line.is_empty() {
                return String::new();
            }
            format!("\n  docs: {}", truncate(first_line, 120))
        }
        _ => String::new(),
    }
}

/// Truncate a string to `max` chars, appending "..." if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Find a valid char boundary at or before max
        let end = s
            .char_indices()
            .take_while(|&(i, _)| i + 3 <= max) // leave room for "..."
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}...", &s[..end])
    }
}

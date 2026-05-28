use crate::model::CodeGraph;
use serde_json::{Value, json};

pub fn quality(graph: &CodeGraph) -> Value {
    let stats = graph.stats();
    let definite = stats
        .by_certainty
        .get("definite")
        .copied()
        .unwrap_or_default();
    let confirmed = stats
        .by_certainty
        .get("confirmed")
        .copied()
        .unwrap_or_default();
    let inferred = stats
        .by_certainty
        .get("inferred")
        .copied()
        .unwrap_or_default();
    let possible = stats
        .by_certainty
        .get("possible")
        .copied()
        .unwrap_or_default();
    let trusted = definite + confirmed;
    let scored_edges = trusted + inferred + possible;
    let confidence = if scored_edges == 0 {
        0
    } else {
        ((trusted * 100) + (inferred * 55) + (possible * 25)) / scored_edges
    };
    let semantic_enabled = graph
        .semantic
        .as_ref()
        .map(|info| info.enabled)
        .unwrap_or_default();
    let mir_enabled = graph
        .mir
        .as_ref()
        .map(|info| info.enabled)
        .unwrap_or_default();
    json!({
        "kind": "quality",
        "score": confidence,
        "interpretation": quality_label(confidence),
        "stats": stats,
        "trusted_edges": trusted,
        "uncertain_edges": inferred + possible,
        "warnings": graph.warnings,
        "recommendations": quality_recommendations(graph, confidence, semantic_enabled, mir_enabled, possible)
    })
}

fn quality_label(score: usize) -> &'static str {
    match score {
        85..=100 => "high",
        65..=84 => "medium",
        1..=64 => "low",
        _ => "empty",
    }
}

fn quality_recommendations(
    graph: &CodeGraph,
    score: usize,
    semantic_enabled: bool,
    mir_enabled: bool,
    possible: usize,
) -> Vec<String> {
    let mut items = Vec::new();
    if !semantic_enabled {
        items.push(
            "Run `index --semantic` to confirm more call edges with rust-analyzer.".to_string(),
        );
    }
    if !mir_enabled {
        items.push("Run `index --mir` when lowered call confirmation matters.".to_string());
    }
    if possible > graph.edges.len().saturating_div(10).max(1) {
        items.push(
            "Many edges are possible dispatch candidates; treat trait-call paths as hypotheses."
                .to_string(),
        );
    }
    if !graph.warnings.is_empty() {
        items.push("Inspect warnings before trusting missing or sparse regions.".to_string());
    }
    if score < 65 {
        items.push(
            "Use source reads for final confirmation; this graph has a low confidence score."
                .to_string(),
        );
    }
    items
}

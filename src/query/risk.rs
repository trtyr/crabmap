use crate::model::CodeGraph;
use crate::test_impact;
use anyhow::Result;
use serde_json::{Value, json};

use super::commands;
use super::find::require_unique_node;
use super::index::QueryIndex;
use super::traversal::node_value;

/// Comprehensive risk assessment for modifying a symbol.
/// Combines impact analysis + test impact into one actionable report.
pub fn risk(graph: &CodeGraph, name: &str, depth: usize, limit: usize) -> Result<Value> {
    let index = QueryIndex::new(graph);
    let node = require_unique_node(graph, name, "symbol")?;

    // 1. Get impact analysis (already has risk scoring)
    let impact = commands::impact(graph, name, depth, limit)?;

    // 2. Get test impact
    let test_info = test_impact::tests(graph, Some(name), limit);

    // 3. Extract risk info from impact
    let risk_info = impact.get("risk").cloned().unwrap_or(json!({}));
    let risk_score = risk_info
        .get("score")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let risk_level = risk_info
        .get("level")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let factors = risk_info.get("factors").cloned().unwrap_or(json!({}));

    // 4. Extract test candidates
    let candidate_tests = test_info
        .get("candidate_tests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // 5. Generate recommendation
    let recommendation = generate_recommendation(risk_level, &factors, candidate_tests.len());

    // 6. Generate suggested test commands
    let suggested_commands = generate_test_commands(name, risk_level, &candidate_tests, graph);

    Ok(json!({
        "kind": "risk",
        "root": node_value(&index, node),
        "risk": {
            "score": risk_score,
            "level": risk_level,
            "factors": factors,
            "recommendation": recommendation
        },
        "impact_summary": {
            "files_affected": impact.get("files_affected").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0),
            "direct_callers": impact.get("call_sites").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0),
            "dependency_count": impact.get("dependencies").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0)
        },
        "test_coverage": {
            "candidate_tests": candidate_tests.len(),
            "tests": candidate_tests
        },
        "suggested_commands": suggested_commands,
        "change_hints": impact.get("change_hints").cloned().unwrap_or(json!([]))
    }))
}

fn generate_recommendation(risk_level: &str, factors: &Value, test_count: usize) -> String {
    let file_count = factors
        .get("files_affected")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let is_pub = factors
        .get("is_public")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    match risk_level {
        "low" => {
            if test_count == 0 {
                "Low risk, no tests found — safe to change, consider adding a test".to_string()
            } else {
                format!(
                    "Low risk — run {} test(s) to verify, then ship",
                    test_count
                )
            }
        }
        "medium" => {
            let mut parts = vec![format!(
                "Medium risk — {} file(s) affected",
                file_count
            )];
            if is_pub {
                parts.push("public API change".to_string());
            }
            if test_count > 0 {
                parts.push(format!("run {} test(s) before merging", test_count));
            } else {
                parts.push("no tests found — consider adding one".to_string());
            }
            parts.join(", ")
        }
        "high" => {
            let mut parts = vec![format!(
                "High risk — {} file(s) affected",
                file_count
            )];
            if is_pub {
                parts.push("public API with callers".to_string());
            }
            parts.push("review all call sites manually".to_string());
            if test_count > 0 {
                parts.push(format!("run {} test(s) + full suite", test_count));
            } else {
                parts.push("NO tests found — high priority to add".to_string());
            }
            parts.join(", ")
        }
        "critical" => {
            format!(
                "Critical risk — core infrastructure ({} files, {} callers). \
                 Staged rollout recommended. Review every call site. \
                 Run full test suite + integration tests.",
                file_count,
                factors.get("direct_callers").and_then(Value::as_u64).unwrap_or(0)
            )
        }
        _ => "Unknown risk level — proceed with caution".to_string(),
    }
}

fn generate_test_commands(
    _name: &str,
    risk_level: &str,
    candidate_tests: &[Value],
    graph: &CodeGraph,
) -> Vec<String> {
    let mut commands = Vec::new();

    // If we have specific test candidates, suggest running them
    if !candidate_tests.is_empty() {
        // Group tests by file to suggest targeted test runs
        let mut test_files: Vec<String> = candidate_tests
            .iter()
            .filter_map(|t| {
                t.get("node")
                    .and_then(|n| n.get("file"))
                    .and_then(Value::as_str)
                    .map(|f| f.to_string())
            })
            .collect();
        test_files.sort();
        test_files.dedup();

        if !test_files.is_empty() {
            // Try to extract test function names for targeted runs
            let test_names: Vec<&str> = candidate_tests
                .iter()
                .filter_map(|t| {
                    t.get("node")
                        .and_then(|n| n.get("name"))
                        .and_then(Value::as_str)
                })
                .take(5)
                .collect();

            if !test_names.is_empty() {
                commands.push(format!(
                    "cargo test {}",
                    test_names.join(" ")
                ));
            }
        }
    }

    // Based on risk level, suggest broader test commands
    match risk_level {
        "low" => {
            // Targeted tests are enough
        }
        "medium" => {
            commands.push("cargo test".to_string());
        }
        "high" | "critical" => {
            commands.push("cargo test".to_string());
            commands.push("cargo clippy -- -D warnings".to_string());
            // Check if there are integration tests
            let has_integration_tests = graph.nodes.iter().any(|n| {
                n.file
                    .as_deref()
                    .is_some_and(|f| f.contains("tests/") && f.contains("test"))
            });
            if has_integration_tests {
                commands.push("cargo test --test '*'".to_string());
            }
        }
        _ => {}
    }

    commands
}

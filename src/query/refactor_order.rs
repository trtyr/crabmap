use crate::model::{CodeGraph, EdgeKind};
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use super::commands::impact;
use super::find::find_nodes;

/// Compute topological refactoring order for a set of symbols.
///
/// Given N symbols, builds a dependency subgraph and returns the order
/// in which they should be refactored (dependencies first).
/// Detects cycles and reports risk per symbol.
pub fn refactor_order(graph: &CodeGraph, names: &[String], limit: usize) -> Result<Value> {
    // 1. Resolve all symbol names to nodes
    let mut resolved: Vec<(String, &crate::model::Node)> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    for name in names {
        let matches = find_nodes(graph, name);
        if matches.is_empty() {
            unresolved.push(name.clone());
        } else {
            // Take first match (prefer function/method)
            let node = matches
                .iter()
                .find(|n| {
                    matches!(
                        n.kind,
                        crate::model::NodeKind::Function
                            | crate::model::NodeKind::Method
                            | crate::model::NodeKind::Constructor
                    )
                })
                .or_else(|| matches.first())
                .unwrap();
            resolved.push((name.clone(), node));
        }
    }

    // 2. Build dependency edges between resolved symbols
    //    dep_edge(A → B) means "A depends on B" (B must be refactored before A)
    let node_ids: BTreeSet<&str> = resolved.iter().map(|(_, n)| n.id.as_str()).collect();
    let id_to_name: HashMap<&str, &str> = resolved
        .iter()
        .map(|(name, node)| (node.id.as_str(), name.as_str()))
        .collect();

    // Build adjacency: adj[to] = set of nodes that call/depend on `to`
    // (incoming edges for Kahn's algorithm in-degree computation)
    // reverse_adj[from] = set of nodes that `from` calls/depends on
    let mut adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut reverse_adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (_, node) in &resolved {
        adj.entry(node.name.clone()).or_default();
        reverse_adj.entry(node.name.clone()).or_default();
    }

    // Check edges between resolved symbols
    for edge in &graph.edges {
        if !matches!(
            edge.kind,
            EdgeKind::Calls | EdgeKind::UsesType | EdgeKind::Returns | EdgeKind::Imports
        ) {
            continue;
        }
        let from_id = edge.from.as_str();
        let to_id = edge.to.as_str();
        // Only consider edges between our resolved symbols
        if node_ids.contains(from_id) && node_ids.contains(to_id) {
            if let (Some(from_name), Some(to_name)) =
                (id_to_name.get(from_id), id_to_name.get(to_id))
            {
                if from_name != to_name {
                    // edge: from → to (from depends on to)
                    // adj[to] += from (incoming edge to `to`)
                    adj.entry(to_name.to_string())
                        .or_default()
                        .insert(from_name.to_string());
                    // reverse_adj[from] += to (outgoing edge from `from`)
                    reverse_adj
                        .entry(from_name.to_string())
                        .or_default()
                        .insert(to_name.to_string());
                }
            }
        }
    }

    // 3. Topological sort (Kahn's algorithm)
    let (order, cycles) = topological_sort(&adj);

    // 4. For each symbol in order, compute individual risk
    let mut steps: Vec<Value> = Vec::new();
    for (i, name) in order.iter().enumerate() {
        let node = resolved
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, node)| *node);

        let (risk_score, risk_level, files_affected) = if let Some(node) = node {
            match impact(graph, &node.name, 2, limit) {
                Ok(impact_val) => {
                    let score = impact_val
                        .get("risk")
                        .and_then(|r| r.get("score"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize;
                    let level = impact_val
                        .get("risk")
                        .and_then(|r| r.get("level"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string();
                    let files = impact_val
                        .get("files_affected")
                        .and_then(Value::as_array)
                        .map(|a| a.len())
                        .unwrap_or(0);
                    (score, level, files)
                }
                Err(_) => (0, "unknown".to_string(), 0),
            }
        } else {
            (0, "unknown".to_string(), 0)
        };

        let depends_on = reverse_adj
            .get(name)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();
        let depended_by = adj
            .get(name)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();

        steps.push(json!({
            "step": i + 1,
            "symbol": name,
            "risk": {
                "score": risk_score,
                "level": risk_level,
                "files_affected": files_affected
            },
            "depends_on": depends_on,
            "depended_by": depended_by,
            "reason": generate_step_reason(i, order.len(), &depends_on, &depended_by, &risk_level)
        }));
    }

    // 5. Build cycle info
    let cycle_info: Vec<Value> = cycles
        .iter()
        .map(|cycle| {
            json!({
                "symbols": cycle,
                "length": cycle.len(),
                "hint": if cycle.len() == 2 {
                    format!("Bidirectional dependency between {} and {} — extract shared interface to break cycle", &cycle[0], &cycle[1])
                } else {
                    format!("Circular dependency among {} symbols — break at weakest link", cycle.len())
                }
            })
        })
        .collect();

    // 6. Overall summary
    let has_cycles = !cycles.is_empty();
    let max_risk = steps
        .iter()
        .filter_map(|s| s.get("risk")?.get("score")?.as_u64())
        .max()
        .unwrap_or(0);

    Ok(json!({
        "kind": "refactor_order",
        "input_count": names.len(),
        "resolved_count": resolved.len(),
        "unresolved": unresolved,
        "order": steps,
        "cycles": cycle_info,
        "has_cycles": has_cycles,
        "max_risk_score": max_risk,
        "summary": generate_summary(&order, &cycles, has_cycles, max_risk)
    }))
}

/// Kahn's algorithm for topological sort.
/// Returns (sorted_order, detected_cycles).
fn topological_sort(adj: &BTreeMap<String, BTreeSet<String>>) -> (Vec<String>, Vec<Vec<String>>) {
    let all_nodes: BTreeSet<String> = adj.keys().cloned().collect();

    // Compute in-degree
    let mut in_degree: BTreeMap<String, usize> = BTreeMap::new();
    for node in &all_nodes {
        in_degree.entry(node.clone()).or_insert(0);
    }
    for (_, neighbors) in adj {
        for neighbor in neighbors {
            *in_degree.entry(neighbor.clone()).or_insert(0) += 1;
        }
    }

    // Queue nodes with in-degree 0
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(name, _)| name.clone())
        .collect();

    let mut order: Vec<String> = Vec::new();
    let mut remaining = in_degree.clone();

    while let Some(current) = queue.pop_front() {
        order.push(current.clone());
        if let Some(neighbors) = adj.get(&current) {
            for neighbor in neighbors {
                if let Some(deg) = remaining.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    // Detect cycles among remaining nodes
    let mut cycles: Vec<Vec<String>> = Vec::new();
    let remaining_nodes: BTreeSet<String> = remaining
        .iter()
        .filter(|(_, deg)| **deg > 0)
        .map(|(name, _)| name.clone())
        .collect();

    if !remaining_nodes.is_empty() {
        // Find strongly connected components using DFS
        let sccs = find_sccs(adj, &remaining_nodes);
        for scc in sccs {
            if scc.len() > 1 {
                cycles.push(scc);
            } else if let Some(name) = scc.first() {
                // Self-loop check
                if adj.get(name).is_some_and(|neighbors| neighbors.contains(name)) {
                    cycles.push(scc);
                }
            }
        }
    }

    (order, cycles)
}

/// Find strongly connected components using Tarjan's algorithm (simplified).
fn find_sccs(adj: &BTreeMap<String, BTreeSet<String>>, nodes: &BTreeSet<String>) -> Vec<Vec<String>> {
    let mut index = 0usize;
    let mut stack: Vec<String> = Vec::new();
    let mut on_stack: BTreeSet<String> = BTreeSet::new();
    let mut indices: BTreeMap<String, usize> = BTreeMap::new();
    let mut lowlinks: BTreeMap<String, usize> = BTreeMap::new();
    let mut sccs: Vec<Vec<String>> = Vec::new();

    for node in nodes {
        if !indices.contains_key(node) {
            strongconnect(
                node,
                adj,
                &mut index,
                &mut stack,
                &mut on_stack,
                &mut indices,
                &mut lowlinks,
                &mut sccs,
            );
        }
    }

    sccs
}

fn strongconnect(
    v: &str,
    adj: &BTreeMap<String, BTreeSet<String>>,
    index: &mut usize,
    stack: &mut Vec<String>,
    on_stack: &mut BTreeSet<String>,
    indices: &mut BTreeMap<String, usize>,
    lowlinks: &mut BTreeMap<String, usize>,
    sccs: &mut Vec<Vec<String>>,
) {
    indices.insert(v.to_string(), *index);
    lowlinks.insert(v.to_string(), *index);
    *index += 1;
    stack.push(v.to_string());
    on_stack.insert(v.to_string());

    if let Some(neighbors) = adj.get(v) {
        for w in neighbors {
            if !indices.contains_key(w.as_str()) {
                strongconnect(w, adj, index, stack, on_stack, indices, lowlinks, sccs);
                let w_low = *lowlinks.get(w.as_str()).unwrap_or(&0);
                let v_low = lowlinks.get_mut(v).unwrap();
                *v_low = (*v_low).min(w_low);
            } else if on_stack.contains(w.as_str()) {
                let w_idx = *indices.get(w.as_str()).unwrap();
                let v_low = lowlinks.get_mut(v).unwrap();
                *v_low = (*v_low).min(w_idx);
            }
        }
    }

    // If v is a root node, pop the SCC
    if lowlinks.get(v) == indices.get(v) {
        let mut scc: Vec<String> = Vec::new();
        loop {
            let w = stack.pop().unwrap();
            on_stack.remove(&w);
            scc.push(w.clone());
            if w == v {
                break;
            }
        }
        sccs.push(scc);
    }
}

fn generate_step_reason(
    step: usize,
    total: usize,
    depends_on: &[String],
    depended_by: &[String],
    risk_level: &str,
) -> String {
    let mut parts = Vec::new();

    if step == 0 {
        parts.push("Start here — no dependencies on other symbols in the set".to_string());
    } else if step == total - 1 {
        parts.push("Last — everything else is already changed".to_string());
    }

    if depends_on.is_empty() {
        parts.push("leaf node — no outgoing dependencies".to_string());
    } else {
        parts.push(format!(
            "depends on: {} (already changed)",
            depends_on.join(", ")
        ));
    }

    if !depended_by.is_empty() {
        parts.push(format!(
            "{} downstream symbol(s) waiting",
            depended_by.len()
        ));
    }

    match risk_level {
        "critical" => parts.push("⚠ critical risk — extra caution needed".to_string()),
        "high" => parts.push("⚠ high risk — review all call sites".to_string()),
        _ => {}
    }

    parts.join("; ")
}

fn generate_summary(
    order: &[String],
    cycles: &[Vec<String>],
    has_cycles: bool,
    max_risk: u64,
) -> String {
    let mut parts = Vec::new();

    parts.push(format!(
        "Refactoring {} symbols in topological order",
        order.len()
    ));

    if has_cycles {
        let total_in_cycles: usize = cycles.iter().map(|c| c.len()).sum();
        parts.push(format!(
            "⚠ {} cycle(s) detected involving {} symbol(s) — break cycles before proceeding",
            cycles.len(),
            total_in_cycles
        ));
    }

    if max_risk >= 15 {
        parts.push(
            "⚠ Critical risk symbols detected — consider staged rollout".to_string(),
        );
    } else if max_risk >= 8 {
        parts.push("High risk symbols present — review call sites carefully".to_string());
    }

    parts.join(". ")
}

use crate::model::{CodeGraph, EdgeKind, NodeKind};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

pub fn health(graph: &CodeGraph, limit: usize) -> Value {
    let file_deps = file_dependencies(graph);
    let all_cycles = cycles(&file_deps, usize::MAX);
    let all_god_modules = god_modules(graph, usize::MAX);
    let all_possible_dead = possible_dead_public_symbols(graph, usize::MAX);
    let cycles = all_cycles.iter().take(limit).cloned().collect::<Vec<_>>();
    let god_modules = all_god_modules
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let hot_symbols = hot_symbols(graph, limit);
    let possible_dead = all_possible_dead
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let score = score(
        all_cycles.len(),
        all_god_modules.len(),
        all_possible_dead.len(),
        graph.warnings.len(),
    );
    json!({
        "kind": "health",
        "score": score,
        "interpretation": label(score),
        "counts": {
            "cycles": all_cycles.len(),
            "god_modules": all_god_modules.len(),
            "possible_dead_public_symbols": all_possible_dead.len(),
            "warnings": graph.warnings.len()
        },
        "cycles": cycles,
        "god_modules": god_modules,
        "hot_symbols": hot_symbols,
        "possible_dead_public_symbols": possible_dead,
        "warnings": graph.warnings
    })
}

fn file_dependencies(graph: &CodeGraph) -> BTreeMap<String, BTreeSet<String>> {
    let nodes = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut deps = BTreeMap::<String, BTreeSet<String>>::new();
    for edge in &graph.edges {
        if !matches!(
            edge.kind,
            EdgeKind::Calls | EdgeKind::Imports | EdgeKind::UsesType | EdgeKind::Returns
        ) {
            continue;
        }
        let Some(from) = nodes
            .get(edge.from.as_str())
            .and_then(|node| node.file.as_deref())
        else {
            continue;
        };
        let Some(to) = nodes
            .get(edge.to.as_str())
            .and_then(|node| node.file.as_deref())
        else {
            continue;
        };
        if from != to {
            deps.entry(from.to_string())
                .or_default()
                .insert(to.to_string());
        }
    }
    deps
}

fn cycles(deps: &BTreeMap<String, BTreeSet<String>>, limit: usize) -> Vec<Value> {
    let mut result = Vec::new();
    for start in deps.keys() {
        if result.len() >= limit {
            break;
        }
        for next in deps.get(start).into_iter().flatten() {
            if let Some(path) = path_to(deps, next, start, 6) {
                let mut cycle = vec![start.clone()];
                cycle.extend(path);
                if cycle.len() > 2 {
                    result.push(json!({ "files": cycle }));
                    break;
                }
            }
        }
    }
    result
}

fn path_to(
    deps: &BTreeMap<String, BTreeSet<String>>,
    start: &str,
    target: &str,
    max_depth: usize,
) -> Option<Vec<String>> {
    let mut seen = BTreeSet::from([start.to_string()]);
    let mut queue = VecDeque::from([(start.to_string(), vec![start.to_string()])]);
    while let Some((current, path)) = queue.pop_front() {
        if path.len() > max_depth {
            continue;
        }
        if current == target {
            return Some(path);
        }
        for next in deps.get(&current).into_iter().flatten() {
            if seen.insert(next.clone()) {
                let mut next_path = path.clone();
                next_path.push(next.clone());
                queue.push_back((next.clone(), next_path));
            }
        }
    }
    None
}

fn god_modules(graph: &CodeGraph, limit: usize) -> Vec<Value> {
    let mut files = BTreeMap::<String, FileStats>::new();
    for node in &graph.nodes {
        let Some(file) = node.file.as_deref() else {
            continue;
        };
        let stats = files.entry(file.to_string()).or_default();
        stats.nodes += 1;
        if !matches!(
            node.kind,
            NodeKind::File | NodeKind::Module | NodeKind::Project | NodeKind::Crate
        ) {
            stats.symbols += 1;
        }
        if let Some(lines) = node.metrics.get("lines") {
            stats.lines = stats.lines.max(*lines);
        }
    }
    let mut items = files
        .into_iter()
        .filter(|(_, stats)| stats.symbols >= 40 || stats.lines >= 500)
        .map(|(file, stats)| {
            json!({
                "file": file,
                "lines": stats.lines,
                "symbols": stats.symbols,
                "nodes": stats.nodes,
                "reason": if stats.symbols >= 40 { "many symbols" } else { "large file" }
            })
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|item| {
        std::cmp::Reverse(
            item.get("symbols")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                + item
                    .get("lines")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
        )
    });
    items.into_iter().take(limit).collect()
}

fn hot_symbols(graph: &CodeGraph, limit: usize) -> Vec<Value> {
    let mut degree = HashMap::<&str, usize>::new();
    for edge in &graph.edges {
        *degree.entry(edge.from.as_str()).or_default() += edge.weight;
        *degree.entry(edge.to.as_str()).or_default() += edge.weight;
    }
    let mut items = graph
        .nodes
        .iter()
        .filter(|node| {
            !matches!(
                node.kind,
                NodeKind::Project | NodeKind::Crate | NodeKind::File | NodeKind::Module
            )
        })
        .map(|node| {
            (
                degree.get(node.id.as_str()).copied().unwrap_or_default(),
                node,
            )
        })
        .filter(|(degree, _)| *degree >= 20)
        .collect::<Vec<_>>();
    items.sort_by_key(|(degree, node)| (std::cmp::Reverse(*degree), node.qualified_name.clone()));
    items
        .into_iter()
        .take(limit)
        .map(|(degree, node)| {
            json!({
                "degree": degree,
                "kind": node.kind.as_str(),
                "name": node.name,
                "qualified_name": node.qualified_name,
                "file": node.file,
                "range": node.range
            })
        })
        .collect()
}

fn possible_dead_public_symbols(graph: &CodeGraph, limit: usize) -> Vec<Value> {
    let mut inbound = HashMap::<&str, usize>::new();
    for edge in &graph.edges {
        if matches!(
            edge.kind,
            EdgeKind::Calls | EdgeKind::UsesType | EdgeKind::Returns | EdgeKind::PossibleDispatch
        ) {
            *inbound.entry(edge.to.as_str()).or_default() += 1;
        }
    }
    graph
        .nodes
        .iter()
        .filter(|node| node.visibility.as_deref().is_some_and(|visibility| visibility.starts_with("pub")))
        .filter(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method | NodeKind::Struct | NodeKind::Enum | NodeKind::Trait))
        .filter(|node| inbound.get(node.id.as_str()).copied().unwrap_or_default() == 0)
        .take(limit)
        .map(|node| {
            json!({
                "kind": node.kind.as_str(),
                "name": node.name,
                "qualified_name": node.qualified_name,
                "file": node.file,
                "range": node.range,
                "note": "No inbound call/type/reference edge in the current graph; verify before deleting."
            })
        })
        .collect()
}

fn score(cycles: usize, god_modules: usize, possible_dead: usize, warnings: usize) -> usize {
    100usize
        .saturating_sub(cycles.min(10) * 5)
        .saturating_sub(god_modules.min(10) * 3)
        .saturating_sub(possible_dead.min(10))
        .saturating_sub(warnings.min(10) * 2)
}

fn label(score: usize) -> &'static str {
    match score {
        85..=100 => "high",
        65..=84 => "medium",
        1..=64 => "low",
        _ => "critical",
    }
}

#[derive(Default)]
struct FileStats {
    lines: usize,
    nodes: usize,
    symbols: usize,
}

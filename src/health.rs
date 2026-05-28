use crate::model::{CodeGraph, EdgeCertainty, EdgeKind, NodeKind};
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
    // Collect methods owned by impl blocks (trait impls) to exclude from cycle detection.
    // These are unreliable because the analyzer resolves all `.from()` to one impl, etc.
    let impl_methods: BTreeSet<&str> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::HasMethod && e.from.starts_with("impl:"))
        .filter_map(|e| nodes.get(e.to.as_str()).map(|n| n.id.as_str()))
        .collect();
    let mut deps = BTreeMap::<String, BTreeSet<String>>::new();
    for edge in &graph.edges {
        // Only use high-confidence edges for cycle detection.
        // Method calls are unreliable due to name-based resolution
        // (e.g., all `.from()` resolves to one `From` impl).
        if !matches!(edge.kind, EdgeKind::Calls | EdgeKind::Imports) {
            continue;
        }
        // Skip inferred/possible edges (from trait dispatch or method resolution)
        if matches!(
            edge.certainty,
            EdgeCertainty::Inferred | EdgeCertainty::Possible
        ) {
            continue;
        }
        // Skip calls to trait impl methods (misresolved generic names like from/into/clone)
        if impl_methods.contains(edge.to.as_str()) {
            continue;
        }
        // Skip edges from/to module/file declaration nodes (mod declarations, not code deps)
        let from_node = nodes.get(edge.from.as_str());
        let to_node = nodes.get(edge.to.as_str());
        match (from_node, to_node) {
            (Some(f), Some(t))
                if matches!(
                    f.kind,
                    NodeKind::Module | NodeKind::File | NodeKind::Project | NodeKind::Crate
                ) || matches!(
                    t.kind,
                    NodeKind::Module | NodeKind::File | NodeKind::Project | NodeKind::Crate
                ) =>
            {
                continue;
            }
            _ => {}
        }
        let (Some(from), Some(to)) = (from_node.and_then(|n| n.file.as_deref()), to_node.and_then(|n| n.file.as_deref())) else {
            continue;
        };
        let from_module = module_path(from);
        let to_module = module_path(to);
        if from_module != to_module {
            deps.entry(from_module.to_string())
                .or_default()
                .insert(to_module.to_string());
        }
    }
    deps
}

/// Collapse sub-module file paths to their parent module directory.
/// e.g. "src/web/server.rs" → "src/web", "src/model.rs" → "src/model.rs"
fn module_path(file: &str) -> &str {
    let path = std::path::Path::new(file);
    if let Some(parent) = path.parent() {
        let parent_str = parent.to_string_lossy();
        // If parent is "src" or ".", use the full file path
        if parent_str == "src" || parent_str == "." || parent_str.is_empty() {
            return file;
        }
        // Otherwise collapse to the module directory
        return Box::leak(parent_str.into_owned().into_boxed_str());
    }
    file
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
    let mut modules = BTreeMap::<String, ModuleStats>::default();
    for node in &graph.nodes {
        let Some(file) = node.file.as_deref() else {
            continue;
        };
        let key = module_path(file).to_string();
        let stats = modules.entry(key).or_default();
        stats.nodes += 1;
        match node.kind {
            // Count "meaningful" symbols (logic-bearing code)
            NodeKind::Function | NodeKind::Method | NodeKind::Trait | NodeKind::Impl | NodeKind::Macro => {
                stats.meaningful += 1;
                stats.symbols += 1;
            }
            // Structs and enums count as symbols but not meaningful (they're type defs)
            NodeKind::Struct | NodeKind::Enum => {
                stats.symbols += 1;
            }
            // Fields, variants, etc. don't count toward god module threshold
            NodeKind::File | NodeKind::Module | NodeKind::Project | NodeKind::Crate => {}
            _ => {
                stats.symbols += 1;
            }
        }
        if let Some(lines) = node.metrics.get("lines") {
            stats.lines += lines;
        }
    }
    let mut items = modules
        .into_iter()
        .filter(|(_, stats)| stats.meaningful >= 40 || stats.lines >= 1200)
        .map(|(module, stats)| {
            json!({
                "module": module,
                "lines": stats.lines,
                "symbols": stats.symbols,
                "meaningful": stats.meaningful,
                "nodes": stats.nodes,
                "reason": if stats.meaningful >= 40 { "many meaningful symbols" } else { "large module" }
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
                "range": node.range,
                "signature": node.signature,
                "docs": node.docs
            })
        })
        .collect()
}

fn possible_dead_public_symbols(graph: &CodeGraph, limit: usize) -> Vec<Value> {
    let mut connected = BTreeSet::<&str>::new();
    for edge in &graph.edges {
        // Any edge touching a symbol means it's connected to the graph
        connected.insert(edge.from.as_str());
        connected.insert(edge.to.as_str());
    }
    graph
        .nodes
        .iter()
        .filter(|node| node.visibility.as_deref() == Some("pub"))
        .filter(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method | NodeKind::Struct | NodeKind::Enum | NodeKind::Trait))
        // Exclude test fixtures
        .filter(|node| !node.file.as_deref().is_some_and(|f| f.contains("fixtures")))
        // A symbol is only dead if it has zero edges of any kind
        .filter(|node| !connected.contains(node.id.as_str()))
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
struct ModuleStats {
    lines: usize,
    nodes: usize,
    symbols: usize,
    meaningful: usize,
}

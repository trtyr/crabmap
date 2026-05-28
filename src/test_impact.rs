use crate::model::{CodeGraph, EdgeKind, NodeKind};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

pub fn tests(graph: &CodeGraph, symbol: Option<&str>, limit: usize) -> Value {
    let targets = graph
        .project
        .packages
        .iter()
        .flat_map(|package| {
            package.targets.iter().map(|target| {
                json!({
                    "package": package.name,
                    "name": target.name,
                    "kind": target.kind,
                    "src_path": target.src_path
                })
            })
        })
        .collect::<Vec<_>>();

    if let Some(sym) = symbol {
        // Find the target node
        let matches: Vec<_> = graph
            .nodes
            .iter()
            .filter(|node| {
                node.name == sym
                    || node.qualified_name == sym
                    || node.qualified_name.ends_with(&format!("::{}", sym))
            })
            .collect();

        if matches.is_empty() || matches.len() > 1 {
            // Fallback to static discovery if ambiguous or not found
            let tests = static_test_discovery(graph, Some(sym), limit);
            return json!({
                "kind": "tests",
                "query": symbol,
                "targets": targets,
                "candidate_tests": tests,
                "note": "Symbol not found or ambiguous — fell back to static test discovery."
            });
        }

        let target_node = matches[0];

        // Build reverse call graph: for each Calls edge B->A, A is called by B
        // rev_adj[A] = set of B that call A
        let mut rev_adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in &graph.edges {
            if edge.kind == EdgeKind::Calls {
                rev_adj
                    .entry(edge.to.as_str())
                    .or_default()
                    .push(edge.from.as_str());
            }
        }

        // BFS from target on reverse call graph up to depth 4
        let mut visited: HashMap<&str, usize> = HashMap::new(); // id -> depth
        let mut parent: HashMap<&str, &str> = HashMap::new(); // id -> parent id
        let mut queue = VecDeque::new();
        visited.insert(target_node.id.as_str(), 0);
        queue.push_back((target_node.id.as_str(), 0usize));

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= 4 {
                continue;
            }
            if let Some(callers) = rev_adj.get(current) {
                for &caller_id in callers {
                    if !visited.contains_key(caller_id) {
                        visited.insert(caller_id, depth + 1);
                        parent.insert(caller_id, current);
                        queue.push_back((caller_id, depth + 1));
                    }
                }
            }
        }

        // Build node lookup by id
        let node_by_id: HashMap<&str, &crate::model::Node> = graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n))
            .collect();

        // Filter to test-like functions
        let mut scored_tests: Vec<(f64, &str, Vec<String>)> = Vec::new();
        for (&node_id, &depth_val) in &visited {
            if depth_val == 0 {
                continue; // skip the target itself
            }
            let Some(node) = node_by_id.get(node_id) else {
                continue;
            };
            if !matches!(node.kind, NodeKind::Function | NodeKind::Method) {
                continue;
            }
            let is_test = node
                .file
                .as_deref()
                .is_some_and(|f| f.contains("test"))
                || node.name.starts_with("test_")
                || node.name.ends_with("_test")
                || node
                    .signature
                    .as_deref()
                    .is_some_and(|s| s.contains("# [test]") || s.contains("# [tokio :: test]"));
            if !is_test {
                continue;
            }

            // Build path from target to this test
            let path = build_path(target_node.id.as_str(), node_id, &parent, &node_by_id);
            let score = 1.0 / (depth_val as f64);
            scored_tests.push((score, node_id, path));
        }

        // Sort by score descending
        scored_tests.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let candidate_tests: Vec<Value> = scored_tests
            .into_iter()
            .take(limit)
            .map(|(score, node_id, path)| {
                let node = node_by_id[node_id];
                json!({
                    "node": {
                        "kind": node.kind.as_str(),
                        "name": node.name,
                        "qualified_name": node.qualified_name,
                        "file": node.file,
                        "range": node.range
                    },
                    "score": score,
                    "path": path
                })
            })
            .collect();

        json!({
            "kind": "tests",
            "query": symbol,
            "target": {
                "kind": target_node.kind.as_str(),
                "name": target_node.name,
                "qualified_name": target_node.qualified_name,
                "file": target_node.file,
                "range": target_node.range
            },
            "candidate_tests": candidate_tests,
            "targets": targets,
            "note": "Call-graph-based test impact analysis. Higher score = closer call proximity."
        })
    } else {
        // No symbol — keep existing static discovery
        let tests = static_test_discovery(graph, None, limit);
        json!({
            "kind": "tests",
            "query": symbol,
            "targets": targets,
            "candidate_tests": tests,
            "note": "This is a conservative static hint based on test-like files/names/signatures and Cargo targets."
        })
    }
}

fn static_test_discovery(graph: &CodeGraph, symbol: Option<&str>, limit: usize) -> Vec<Value> {
    graph
        .nodes
        .iter()
        .filter(|node| {
            matches!(node.kind, NodeKind::Function | NodeKind::Method)
        })
        .filter(|node| {
            node.file
                .as_deref()
                .is_some_and(|file| file.contains("test"))
                || node.name.starts_with("test_")
                || node.name.ends_with("_test")
                || node.signature.as_deref().is_some_and(|signature| {
                    signature.contains("# [test]") || signature.contains("# [tokio :: test]")
                })
        })
        .filter(|node| {
            symbol.is_none_or(|symbol| {
                let haystack = format!(
                    "{} {} {}",
                    node.name,
                    node.qualified_name,
                    node.file.as_deref().unwrap_or_default()
                );
                haystack.contains(symbol)
            })
        })
        .map(|node| {
            json!({
                "kind": node.kind.as_str(),
                "name": node.name,
                "qualified_name": node.qualified_name,
                "file": node.file,
                "range": node.range
            })
        })
        .take(limit)
        .collect()
}

fn build_path<'a>(
    target_id: &str,
    from_id: &str,
    parent: &HashMap<&str, &str>,
    node_by_id: &HashMap<&str, &'a crate::model::Node>,
) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = from_id;
    loop {
        let name = node_by_id
            .get(current)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| current.to_string());
        path.push(name);
        if current == target_id {
            break;
        }
        match parent.get(current) {
            Some(&p) => current = p,
            None => break,
        }
    }
    path.reverse();
    path
}

pub fn changed_targets(graph: &CodeGraph, changed_files: &[String]) -> Value {
    let targets = graph
        .project
        .packages
        .iter()
        .flat_map(|package| {
            package.targets.iter().filter_map(|target| {
                changed_files
                    .iter()
                    .any(|file| target.src_path.ends_with(file) || file.starts_with("src/"))
                    .then(|| {
                        json!({
                            "package": package.name,
                            "target": target.name,
                            "kind": target.kind,
                            "src_path": target.src_path
                        })
                    })
            })
        })
        .collect::<Vec<_>>();
    json!({ "targets": targets })
}

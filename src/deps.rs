use crate::model::{CodeGraph, EdgeKind};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub fn deps(graph: &CodeGraph, from: Option<&str>, to: Option<&str>, limit: usize) -> Value {
    let nodes = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut deps = BTreeMap::<(String, String), usize>::new();
    for edge in &graph.edges {
        if !matches!(
            edge.kind,
            EdgeKind::Calls | EdgeKind::Imports | EdgeKind::UsesType | EdgeKind::Returns
        ) {
            continue;
        }
        let Some(from_file) = nodes
            .get(edge.from.as_str())
            .and_then(|node| node.file.as_deref())
        else {
            continue;
        };
        let Some(to_file) = nodes
            .get(edge.to.as_str())
            .and_then(|node| node.file.as_deref())
        else {
            continue;
        };
        let from_cluster = cluster_key(from_file);
        let to_cluster = cluster_key(to_file);
        if from_cluster == to_cluster {
            continue;
        }
        if from.is_some_and(|filter| !from_cluster.contains(filter)) {
            continue;
        }
        if to.is_some_and(|filter| !to_cluster.contains(filter)) {
            continue;
        }
        *deps.entry((from_cluster, to_cluster)).or_default() += edge.weight;
    }

    // Build adjacency map for cluster-level dependency graph
    let mut cluster_adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for ((from_c, to_c), _) in &deps {
        cluster_adj
            .entry(from_c.clone())
            .or_default()
            .insert(to_c.clone());
    }

    // recompile_impact: BFS from each source, find chains >= 3
    let mut recompile_impact: Vec<Value> = Vec::new();
    let all_sources: BTreeSet<String> = deps.keys().map(|(f, _)| f.clone()).collect();
    for source in &all_sources {
        let chain = bfs_chain(&cluster_adj, source, 5);
        if chain.len() >= 3 {
            recompile_impact.push(json!({
                "from": source,
                "to": chain.last().unwrap_or(source),
                "chain": chain,
                "chain_length": chain.len()
            }));
        }
    }

    // total_recompile_modules: find most-connected source, compute transitive closure
    let total_recompile_modules = all_sources
        .iter()
        .map(|source| {
            let closure = bfs_transitive_closure(&cluster_adj, source, 5);
            (source, closure.len())
        })
        .max_by_key(|(_, count)| *count)
        .map(|(_, count)| count)
        .unwrap_or(0);

    let mut items = deps
        .into_iter()
        .map(|((from, to), weight)| json!({ "from": from, "to": to, "weight": weight }))
        .collect::<Vec<_>>();
    items.sort_by_key(|item| {
        std::cmp::Reverse(
            item.get("weight")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        )
    });
    json!({
        "kind": "deps",
        "items": items.into_iter().take(limit).collect::<Vec<_>>(),
        "recompile_impact": recompile_impact,
        "total_recompile_modules": total_recompile_modules
    })
}

pub fn fanout(graph: &CodeGraph, limit: usize) -> Value {
    let nodes = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut outbound = BTreeMap::<String, BTreeSet<String>>::new();
    let mut inbound = BTreeMap::<String, BTreeSet<String>>::new();
    for edge in &graph.edges {
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
            outbound
                .entry(from.to_string())
                .or_default()
                .insert(to.to_string());
            inbound
                .entry(to.to_string())
                .or_default()
                .insert(from.to_string());
        }
    }
    let mut items = outbound
        .keys()
        .chain(inbound.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|file| {
            let out = outbound.get(file).map(BTreeSet::len).unwrap_or_default();
            let inn = inbound.get(file).map(BTreeSet::len).unwrap_or_default();
            json!({ "file": file, "fanout": out, "fanin": inn, "total": out + inn })
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|item| {
        std::cmp::Reverse(
            item.get("total")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        )
    });
    json!({
        "kind": "fanout",
        "items": items.into_iter().take(limit).collect::<Vec<_>>()
    })
}

fn cluster_key(file: &str) -> String {
    let mut parts = file.split('/').collect::<Vec<_>>();
    if parts.first() == Some(&"src") {
        parts.remove(0);
    }
    match parts.as_slice() {
        [] => "root".to_string(),
        [single] => single.trim_end_matches(".rs").to_string(),
        [first, second, ..] if *first == "bin" => format!("bin/{second}"),
        [first, second, ..] => format!("{first}/{second}"),
    }
}

fn bfs_chain(adj: &BTreeMap<String, BTreeSet<String>>, start: &str, max_depth: usize) -> Vec<String> {
    use std::collections::VecDeque;
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    let mut path = vec![start.to_string()];
    visited.insert(start.to_string());
    queue.push_back((start.to_string(), 1usize));
    while let Some((current, depth)) = queue.pop_front() {
        if depth > max_depth {
            break;
        }
        if let Some(neighbors) = adj.get(&current) {
            for next in neighbors {
                if visited.insert(next.clone()) {
                    path.push(next.clone());
                    queue.push_back((next.clone(), depth + 1));
                    break; // follow one branch to get longest chain
                }
            }
        }
    }
    path
}

fn bfs_transitive_closure(adj: &BTreeMap<String, BTreeSet<String>>, start: &str, max_depth: usize) -> BTreeSet<String> {
    use std::collections::VecDeque;
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start.to_string());
    queue.push_back((start.to_string(), 0usize));
    while let Some((current, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        if let Some(neighbors) = adj.get(&current) {
            for next in neighbors {
                if visited.insert(next.clone()) {
                    queue.push_back((next.clone(), depth + 1));
                }
            }
        }
    }
    visited.remove(start);
    visited
}

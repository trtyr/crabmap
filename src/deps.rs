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
        "items": items.into_iter().take(limit).collect::<Vec<_>>()
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

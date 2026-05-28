use crate::model::{CodeGraph, Node, NodeKind};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use super::guide::node_value;
use super::index::AiIndex;

pub fn clusters(graph: &CodeGraph, limit: usize) -> Value {
    let index = AiIndex::new(graph);
    let mut clusters = BTreeMap::<String, Cluster>::new();
    for node in &graph.nodes {
        let Some(file) = node.file.as_deref() else {
            continue;
        };
        let key = cluster_key(file);
        let cluster = clusters.entry(key.clone()).or_insert_with(|| Cluster {
            name: key,
            files: BTreeSet::new(),
            nodes: 0,
            symbols: 0,
            degree: 0,
            hot: Vec::new(),
        });
        cluster.files.insert(file.to_string());
        cluster.nodes += 1;
        if !matches!(
            node.kind,
            NodeKind::File | NodeKind::Module | NodeKind::Project | NodeKind::Crate
        ) {
            cluster.symbols += 1;
        }
        let degree = index.degree(&node.id);
        cluster.degree += degree;
        if !matches!(
            node.kind,
            NodeKind::File | NodeKind::Module | NodeKind::Project | NodeKind::Crate
        ) {
            cluster.hot.push((degree, node));
        }
    }
    let mut items = clusters
        .into_values()
        .map(|mut cluster| {
            cluster.hot.sort_by_key(|(degree, node)| {
                (std::cmp::Reverse(*degree), node.qualified_name.clone())
            });
            json!({
                "name": cluster.name,
                "files": cluster.files.len(),
                "nodes": cluster.nodes,
                "symbols": cluster.symbols,
                "degree": cluster.degree,
                "hot_symbols": cluster.hot.into_iter().take(8).map(|(_, node)| node_value(&index, node)).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|item| {
        std::cmp::Reverse(
            item.get("degree")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        )
    });
    json!({
        "kind": "clusters",
        "items": items.into_iter().take(limit).collect::<Vec<_>>()
    })
}

struct Cluster<'a> {
    name: String,
    files: BTreeSet<String>,
    nodes: usize,
    symbols: usize,
    degree: usize,
    hot: Vec<(usize, &'a Node)>,
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

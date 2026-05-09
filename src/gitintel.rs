use crate::model::CodeGraph;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;

pub fn git_intel(graph: &CodeGraph, project: Option<&Path>, limit: usize) -> Result<Value> {
    let root = repo_root(graph, project)?;
    let log = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["log", "--name-only", "--format=__COMMIT__%an"])
        .output()
        .with_context(|| format!("failed to run git log in {}", root.display()))?;
    if !log.status.success() {
        return Ok(json!({
            "kind": "git",
            "error": String::from_utf8_lossy(&log.stderr)
        }));
    }
    let mut churn = HashMap::<String, usize>::new();
    let mut owners = HashMap::<String, HashMap<String, usize>>::new();
    let mut pairs = HashMap::<(String, String), usize>::new();
    let mut author = String::new();
    let mut files = Vec::new();
    for line in String::from_utf8_lossy(&log.stdout).lines() {
        if let Some(next_author) = line.strip_prefix("__COMMIT__") {
            flush_commit(&mut churn, &mut owners, &mut pairs, &author, &files);
            author = next_author.to_string();
            files.clear();
            continue;
        }
        if line.ends_with(".rs") || line == "Cargo.toml" {
            files.push(line.to_string());
        }
    }
    flush_commit(&mut churn, &mut owners, &mut pairs, &author, &files);
    let degrees = file_degrees(graph);
    let mut hotspots = churn
        .iter()
        .map(|(file, changes)| {
            let degree = degrees.get(file).copied().unwrap_or_else(|| {
                degrees
                    .iter()
                    .find(|(graph_file, _)| file.ends_with(graph_file.as_str()))
                    .map(|(_, degree)| *degree)
                    .unwrap_or_default()
            });
            json!({
                "file": file,
                "changes": changes,
                "degree": degree,
                "risk": changes + degree,
                "owner": top_owner(owners.get(file))
            })
        })
        .collect::<Vec<_>>();
    hotspots.sort_by_key(|item| {
        std::cmp::Reverse(item.get("risk").and_then(Value::as_u64).unwrap_or_default())
    });
    let mut cochange = pairs
        .into_iter()
        .map(|((a, b), count)| json!({ "files": [a, b], "count": count }))
        .collect::<Vec<_>>();
    cochange.sort_by_key(|item| {
        std::cmp::Reverse(
            item.get("count")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        )
    });
    Ok(json!({
        "kind": "git",
        "repo": root,
        "hotspots": hotspots.into_iter().take(limit).collect::<Vec<_>>(),
        "cochange": cochange.into_iter().take(limit).collect::<Vec<_>>()
    }))
}

pub fn repo_root(graph: &CodeGraph, project: Option<&Path>) -> Result<std::path::PathBuf> {
    let root = project.map(Path::to_path_buf).unwrap_or_else(|| {
        if graph.project.root == "." {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        } else {
            std::path::PathBuf::from(&graph.project.root)
        }
    });
    let output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .with_context(|| format!("failed to locate git repo from {}", root.display()))?;
    if !output.status.success() {
        return Ok(root);
    }
    Ok(Path::new(String::from_utf8_lossy(&output.stdout).trim()).to_path_buf())
}

fn flush_commit(
    churn: &mut HashMap<String, usize>,
    owners: &mut HashMap<String, HashMap<String, usize>>,
    pairs: &mut HashMap<(String, String), usize>,
    author: &str,
    files: &[String],
) {
    let mut unique = files.to_vec();
    unique.sort();
    unique.dedup();
    for file in &unique {
        *churn.entry(file.clone()).or_default() += 1;
        if !author.is_empty() {
            *owners
                .entry(file.clone())
                .or_default()
                .entry(author.to_string())
                .or_default() += 1;
        }
    }
    for (index, a) in unique.iter().enumerate() {
        for b in unique.iter().skip(index + 1) {
            *pairs.entry((a.clone(), b.clone())).or_default() += 1;
        }
    }
}

fn top_owner(owners: Option<&HashMap<String, usize>>) -> Option<String> {
    owners.and_then(|owners| {
        owners
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(owner, count)| format!("{owner} ({count})"))
    })
}

fn file_degrees(graph: &CodeGraph) -> BTreeMap<String, usize> {
    let nodes = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.file.clone()))
        .collect::<HashMap<_, _>>();
    let mut degrees = BTreeMap::new();
    for edge in &graph.edges {
        if let Some(Some(file)) = nodes.get(edge.from.as_str()) {
            *degrees.entry(file.clone()).or_default() += edge.weight;
        }
        if let Some(Some(file)) = nodes.get(edge.to.as_str()) {
            *degrees.entry(file.clone()).or_default() += edge.weight;
        }
    }
    degrees
}

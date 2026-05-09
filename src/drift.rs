use crate::analyzer::{self, IndexOptions};
use crate::health;
use crate::model::CodeGraph;
use crate::test_impact;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

pub fn diff(
    graph: &CodeGraph,
    project: Option<&std::path::Path>,
    base: &str,
    limit: usize,
) -> Result<Value> {
    let root = repo_root(graph, project)?;
    let changed_output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["diff", "--name-only", base, "--"])
        .output()
        .with_context(|| format!("failed to run git diff in {}", root.display()))?;
    let changed_files = String::from_utf8_lossy(&changed_output.stdout)
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let base_graph = build_base_graph(&root, base).ok();
    let current_edges = edge_set(graph);
    let (added_edges, removed_edges) = base_graph
        .as_ref()
        .map(|base_graph| {
            let base_edges = edge_set(base_graph);
            (
                current_edges
                    .difference(&base_edges)
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>(),
                base_edges
                    .difference(&current_edges)
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    Ok(json!({
        "kind": "diff",
        "base": base,
        "changed_files": changed_files,
        "added_edges": added_edges,
        "removed_edges": removed_edges,
        "current_health": health::health(graph, limit),
        "changed_targets": test_impact::changed_targets(graph, &String::from_utf8_lossy(&changed_output.stdout).lines().map(ToString::to_string).collect::<Vec<_>>()),
        "note": if base_graph.is_some() { "Compared against a temporary graph built from base." } else { "Could not build base graph; showing changed files and current health only." }
    }))
}

fn repo_root(graph: &CodeGraph, project: Option<&std::path::Path>) -> Result<PathBuf> {
    let root = project.map(PathBuf::from).unwrap_or_else(|| {
        if graph.project.root == "." {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        } else {
            PathBuf::from(&graph.project.root)
        }
    });
    let output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    if !output.status.success() {
        return Ok(root);
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

fn build_base_graph(root: &std::path::Path, base: &str) -> Result<CodeGraph> {
    let temp = tempfile::tempdir().context("failed to create temp dir for base graph")?;
    let archive = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["archive", base])
        .output()
        .with_context(|| format!("failed to archive {base}"))?;
    if !archive.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&archive.stderr));
    }
    let mut tar = Command::new("tar")
        .arg("-x")
        .arg("-C")
        .arg(temp.path())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn tar")?;
    use std::io::Write;
    tar.stdin
        .as_mut()
        .context("failed to open tar stdin")?
        .write_all(&archive.stdout)?;
    let status = tar.wait()?;
    if !status.success() {
        anyhow::bail!("failed to unpack base archive");
    }
    analyzer::index_project(
        temp.path(),
        IndexOptions {
            include_tests: false,
            features: Vec::new(),
            all_features: false,
            no_default_features: false,
            target: None,
            profile: format!("base-{base}"),
            on_progress: None,
        },
    )
}

fn edge_set(graph: &CodeGraph) -> BTreeSet<String> {
    graph
        .edges
        .iter()
        .map(|edge| format!("{} -> {} [{}]", edge.from, edge.to, edge.kind.as_str()))
        .collect()
}

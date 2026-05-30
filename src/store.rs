use crate::model::{CodeGraph, NodeKind, Project};
use anyhow::{Context, Result};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn default_path(path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = path {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()?
        .join(".crabmap")
        .join("crabmap.json.gz"))
}

pub fn default_project_path(project: &Path, output: Option<&Path>) -> Result<PathBuf> {
    if let Some(output) = output {
        return Ok(output.to_path_buf());
    }
    Ok(project
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", project.display()))?
        .join(".crabmap")
        .join("crabmap.json.gz"))
}

fn write_graph(path: &Path, graph: &CodeGraph) -> Result<()> {
    let json = serde_json::to_vec(graph)?;
    let file = std::fs::File::create(path)?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder.write_all(&json)?;
    encoder.finish()?;
    Ok(())
}

fn read_graph(path: &Path) -> Result<CodeGraph> {
    let file = std::fs::File::open(path)?;
    let decoder = GzDecoder::new(file);
    Ok(serde_json::from_reader(decoder)?)
}

pub fn save(path: Option<&Path>, graph: &CodeGraph) -> Result<()> {
    let path = default_path(path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    write_graph(&path, graph).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn save_project(project: &Path, output: Option<&Path>, graph: &CodeGraph) -> Result<PathBuf> {
    let path = default_project_path(project, output)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    write_graph(&path, graph).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

pub fn load(path: Option<&Path>) -> Result<CodeGraph> {
    let path = default_path(path)?;
    if !path.exists() {
        anyhow::bail!(
            "graph file not found: {}\nRun `crabmap index` first to build the code graph.",
            path.display()
        );
    }
    read_graph(&path).with_context(|| format!("failed to read {}", path.display()))
}

pub fn load_many(paths: &[PathBuf]) -> Result<CodeGraph> {
    if !paths.is_empty() {
        let mut graphs = paths
            .iter()
            .map(|path| load(Some(path)))
            .collect::<Result<Vec<_>>>()?;
        return if graphs.len() == 1 {
            Ok(graphs.remove(0))
        } else {
            merge(graphs)
        };
    }
    // No explicit paths: try default → auto-discover → fallback
    let default = default_path(None)?;
    if default.exists() {
        return load(Some(&default));
    }
    if let Some(graphs) = discover_graphs() {
        return load_many(&graphs);
    }
    load(None)
}

fn discover_graphs() -> Option<Vec<PathBuf>> {
    let dir = std::env::current_dir().ok()?.join(".crabmap");
    if !dir.is_dir() {
        return None;
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            path.file_name()?.to_string_lossy().ends_with(".json.gz").then_some(path)
        })
        .collect();
    files.sort();
    files.dedup();
    if files.is_empty() {
        None
    } else {
        Some(files)
    }
}

fn merge(graphs: Vec<CodeGraph>) -> Result<CodeGraph> {
    let total = graphs.len();
    let root = std::env::current_dir()?.display().to_string();
    let mut merged = CodeGraph {
        schema_version: graphs
            .iter()
            .map(|graph| graph.schema_version)
            .max()
            .unwrap_or(2),
        project: Project {
            root,
            workspace_root: ".".to_string(),
            packages: graphs
                .iter()
                .flat_map(|graph| graph.project.packages.clone())
                .collect(),
        },
        nodes: Vec::new(),
        edges: Vec::new(),
        warnings: graphs
            .iter()
            .flat_map(|graph| graph.warnings.clone())
            .collect(),
        semantic: None,
        mir: None,
        profiles: graphs
            .iter()
            .flat_map(|graph| graph.profiles.clone())
            .collect(),
        generated_at_ms: graphs
            .iter()
            .map(|graph| graph.generated_at_ms)
            .max()
            .unwrap_or_default(),
    };
    for (index, graph) in graphs.into_iter().enumerate() {
        let prefix = if total > 1 {
            format!("{}{}", graph_prefix(&graph), index + 1)
        } else {
            graph_prefix(&graph)
        };
        for mut node in graph.nodes {
            let old_id = node.id.clone();
            node.id = format!("{prefix}:{old_id}");
            if node.kind == NodeKind::Project {
                node.name = format!("{prefix}:{}", node.name);
                node.qualified_name = format!("{prefix}:{}", node.qualified_name);
            }
            merged.nodes.push(node);
        }
        for mut edge in graph.edges {
            edge.from = format!("{prefix}:{}", edge.from);
            edge.to = format!("{prefix}:{}", edge.to);
            merged.edges.push(edge);
        }
    }
    Ok(merged)
}

fn graph_prefix(graph: &CodeGraph) -> String {
    graph
        .project
        .packages
        .first()
        .map(|package| package.name.clone())
        .or_else(|| {
            graph
                .project
                .root
                .split(std::path::MAIN_SEPARATOR)
                .next_back()
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "graph".to_string())
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

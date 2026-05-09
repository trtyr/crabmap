use crate::model::{CodeGraph, Edge, EdgeCertainty, EdgeKind, EdgeSource, Location, MirInfo};
use anyhow::{Context, Result};
use cargo_metadata::{CargoOpt, MetadataCommand, TargetKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

struct SymbolIndex {
    exact: HashMap<String, String>,
    by_name: HashMap<String, Vec<String>>,
}

pub fn enrich(graph: &mut CodeGraph, project: &Path, limit: usize) -> Result<()> {
    let profile = graph.profiles.first().cloned();
    let root = workspace_root(graph, project)?;
    let manifest = find_manifest(&root)?;
    let metadata = metadata(&manifest, profile.as_ref())?;
    let symbol_index = SymbolIndex::new(graph);
    let mut scanned_targets = 0;
    let mut scanned_functions = 0;
    let mut enriched_edges = 0;
    let mut confirmed_edges = 0;
    let mut unresolved_items = 0;
    let mut warnings = Vec::new();

    for package in metadata.packages {
        if !metadata.workspace_members.contains(&package.id) {
            continue;
        }
        for target in package.targets {
            let Some(target_args) = target_args(&target) else {
                continue;
            };
            scanned_targets += 1;
            let output = match run_mir(&manifest, profile.as_ref(), &target_args) {
                Ok(output) => output,
                Err(error) => {
                    warnings.push(format!("{}: {error:#}", target.name));
                    continue;
                }
            };
            let mut remaining = limit.saturating_sub(scanned_functions);
            if remaining == 0 {
                break;
            }
            let stats = ingest_mir(
                graph,
                &symbol_index,
                &output,
                &mut remaining,
                profile
                    .as_ref()
                    .map(|profile| profile.name.as_str())
                    .unwrap_or("default"),
            );
            scanned_functions += stats.scanned_functions;
            enriched_edges += stats.enriched_edges;
            confirmed_edges += stats.confirmed_edges;
            unresolved_items += stats.unresolved_items;
        }
    }

    graph.mir = Some(MirInfo {
        provider: "rustc -Z unpretty=mir".to_string(),
        enabled: true,
        scanned_targets,
        scanned_functions,
        enriched_edges,
        confirmed_edges,
        unresolved_items,
        warnings,
    });
    Ok(())
}

fn metadata(
    manifest: &Path,
    profile: Option<&crate::model::BuildProfile>,
) -> Result<cargo_metadata::Metadata> {
    let mut command = MetadataCommand::new();
    command.manifest_path(manifest);
    if let Some(profile) = profile {
        if profile.all_features {
            command.features(CargoOpt::AllFeatures);
        }
        if profile.no_default_features {
            command.features(CargoOpt::NoDefaultFeatures);
        }
        if !profile.features.is_empty() {
            command.features(CargoOpt::SomeFeatures(profile.features.clone()));
        }
        if let Some(target) = &profile.target {
            command.other_options(vec!["--filter-platform".to_string(), target.clone()]);
        }
    }
    command
        .exec()
        .context("failed to run cargo metadata for MIR")
}

fn run_mir(
    manifest: &Path,
    profile: Option<&crate::model::BuildProfile>,
    target_args: &[String],
) -> Result<String> {
    let mut command = Command::new("cargo");
    command
        .env("RUSTC_BOOTSTRAP", "1")
        .arg("rustc")
        .arg("--manifest-path")
        .arg(manifest)
        .args(target_args);
    if let Some(profile) = profile {
        if profile.all_features {
            command.arg("--all-features");
        }
        if profile.no_default_features {
            command.arg("--no-default-features");
        }
        if !profile.features.is_empty() {
            command.arg("--features").arg(profile.features.join(","));
        }
        if let Some(target) = &profile.target {
            command.arg("--target").arg(target);
        }
    }
    let output = command.arg("--").arg("-Z").arg("unpretty=mir").output()?;
    if !output.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn target_args(target: &cargo_metadata::Target) -> Option<Vec<String>> {
    if target.kind.iter().any(|kind| {
        matches!(
            kind,
            TargetKind::Lib | TargetKind::RLib | TargetKind::ProcMacro
        )
    }) {
        return Some(vec!["--lib".to_string()]);
    }
    if target
        .kind
        .iter()
        .any(|kind| matches!(kind, TargetKind::Bin))
    {
        return Some(vec!["--bin".to_string(), target.name.to_string()]);
    }
    None
}

struct MirStats {
    scanned_functions: usize,
    enriched_edges: usize,
    confirmed_edges: usize,
    unresolved_items: usize,
}

fn ingest_mir(
    graph: &mut CodeGraph,
    index: &SymbolIndex,
    output: &str,
    remaining: &mut usize,
    profile: &str,
) -> MirStats {
    let mut stats = MirStats {
        scanned_functions: 0,
        enriched_edges: 0,
        confirmed_edges: 0,
        unresolved_items: 0,
    };
    let mut current_owner = None;
    let mut seen = HashSet::new();
    for line in output.lines() {
        if *remaining == 0 {
            break;
        }
        if let Some(name) = mir_function_name(line) {
            current_owner = index.resolve(&name);
            stats.scanned_functions += 1;
            *remaining = remaining.saturating_sub(1);
            continue;
        }
        let Some(owner) = current_owner.as_ref() else {
            continue;
        };
        let Some(call) = mir_call_name(line) else {
            continue;
        };
        if should_skip_call(&call) {
            continue;
        }
        let Some(target) = index.resolve(&call) else {
            stats.unresolved_items += 1;
            continue;
        };
        if !seen.insert((owner.clone(), target.clone(), call.clone())) {
            continue;
        }
        match add_edge(graph, owner, &target, &call, profile) {
            EdgeResult::Inserted => stats.enriched_edges += 1,
            EdgeResult::Confirmed => stats.confirmed_edges += 1,
            EdgeResult::Skipped => {}
        }
    }
    stats
}

fn mir_function_name(line: &str) -> Option<String> {
    let line = line.trim_start();
    let rest = line.strip_prefix("fn ")?;
    if rest.starts_with("{{") || rest.starts_with('<') && rest.contains(" as Fn") {
        return None;
    }
    Some(normalize_symbol(rest.split('(').next()?.trim()))
}

fn mir_call_name(line: &str) -> Option<String> {
    let line = line.trim();
    if !line.contains(" -> ") {
        return None;
    }
    let before_arrow = line.split(" -> ").next()?.trim();
    let before_args = before_arrow.rsplit_once('(')?.0;
    let callee = before_args
        .rsplit_once('=')
        .map(|(_, value)| value.trim())?;
    Some(normalize_symbol(callee))
}

fn normalize_symbol(value: &str) -> String {
    let value = value.trim();
    if let Some((_, method)) = value.rsplit_once(">::") {
        return method.trim().to_string();
    }
    if let Some((_, method)) = value.rsplit_once("::") {
        return method.trim().to_string();
    }
    value
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim()
        .to_string()
}

fn should_skip_call(call: &str) -> bool {
    matches!(call, "drop" | "clone" | "into" | "from" | "default")
        || call.starts_with("std::")
        || call.starts_with("core::")
}

fn add_edge(graph: &mut CodeGraph, from: &str, to: &str, label: &str, profile: &str) -> EdgeResult {
    if from == to {
        return EdgeResult::Skipped;
    }
    if let Some(edge) = graph
        .edges
        .iter_mut()
        .find(|edge| edge.from == from && edge.to == to && edge.kind == EdgeKind::Calls)
    {
        edge.weight += 1;
        edge.source = EdgeSource::Mir;
        edge.certainty = EdgeCertainty::Confirmed;
        if !edge.profiles.iter().any(|item| item == profile) {
            edge.profiles.push(profile.to_string());
        }
        return EdgeResult::Confirmed;
    }
    graph.edges.push(Edge {
        from: from.to_string(),
        to: to.to_string(),
        kind: EdgeKind::Calls,
        label: Some(label.to_string()),
        evidence: Some(Location {
            file: "mir".to_string(),
            line: 0,
        }),
        weight: 1,
        source: EdgeSource::Mir,
        certainty: EdgeCertainty::Confirmed,
        profiles: vec![profile.to_string()],
    });
    EdgeResult::Inserted
}

enum EdgeResult {
    Inserted,
    Confirmed,
    Skipped,
}

impl SymbolIndex {
    fn new(graph: &CodeGraph) -> Self {
        let mut exact = HashMap::new();
        let mut by_name = HashMap::<String, Vec<String>>::new();
        for node in &graph.nodes {
            exact.insert(node.qualified_name.clone(), node.id.clone());
            by_name
                .entry(node.name.clone())
                .or_default()
                .push(node.id.clone());
        }
        Self { exact, by_name }
    }

    fn resolve(&self, name: &str) -> Option<String> {
        if let Some(id) = self.exact.get(name) {
            return Some(id.clone());
        }
        let short = name.split("::").last().unwrap_or(name);
        self.by_name
            .get(short)
            .filter(|items| items.len() == 1)
            .and_then(|items| items.first())
            .cloned()
    }
}

fn workspace_root(graph: &CodeGraph, project: &Path) -> Result<PathBuf> {
    let project = project.canonicalize()?;
    if graph.project.workspace_root == "." {
        return Ok(project);
    }
    Ok(project.join(&graph.project.workspace_root).canonicalize()?)
}

fn find_manifest(root: &Path) -> Result<PathBuf> {
    if root.join("Cargo.toml").exists() {
        return Ok(root.join("Cargo.toml"));
    }
    for ancestor in root.ancestors() {
        if ancestor.join("Cargo.toml").exists() {
            return Ok(ancestor.join("Cargo.toml"));
        }
    }
    anyhow::bail!("Cargo.toml not found from {}", root.display())
}

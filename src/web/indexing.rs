use super::config::ServeConfig;
use super::helpers::store_current_graph_locked;
use super::state::AppState;
use crate::analyzer::{self, IndexOptions};
use crate::mir;
use crate::model::CodeGraph;
use crate::semantic;
use crate::store;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

pub(crate) fn reindex(state: &Arc<Mutex<AppState>>) {
    {
        let mut state = state.lock().unwrap();
        state.status.indexing = true;
        state.status.last_event = "indexing".to_string();
        state.status.errors.clear();
    }
    let started = SystemTime::now();
    let config = state.lock().unwrap().config.clone();
    // Try loading from existing graph file first
    let result = if let Some(ref graph_path) = config.graph {
        if graph_path.exists() {
            store::load(Some(graph_path))
        } else {
            build_graph(&config)
        }
    } else {
        build_graph(&config)
    };
    let elapsed = started
        .elapsed()
        .map(|value| value.as_millis())
        .unwrap_or(0);
    let mut state = state.lock().unwrap();
    match result {
        Ok(graph) => {
            state.last_mtimes = rust_mtimes(&config.project);
            state.graph = Some(graph);
            // Pre-gzip graph JSON for fast API serving
            if let Ok(json) = serde_json::to_vec(state.graph.as_ref().unwrap()) {
                use flate2::Compression;
                use flate2::write::GzEncoder;
                use std::io::Write;
                let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                if encoder.write_all(&json).is_ok() {
                    state.graph_gz = encoder.finish().ok();
                }
            }
            state.status.last_index_ms = elapsed;
            state.status.last_event = "ready".to_string();
            state.status.errors.clear();
            if let Err(error) = store_current_graph_locked(&state) {
                state.status.errors.push(format!("{error:#}"));
            }
        }
        Err(error) => {
            state.status.last_event = "failed".to_string();
            state.status.errors.push(format!("{error:#}"));
        }
    }
    state.status.indexing = false;
}

fn build_graph(config: &ServeConfig) -> Result<CodeGraph> {
    let mut graph = analyzer::index_project(
        &config.project,
        IndexOptions {
            include_tests: config.include_tests,
            features: config.features.clone(),
            all_features: config.all_features,
            no_default_features: config.no_default_features,
            target: config.target.clone(),
            profile: config.profile.clone(),
            on_progress: None,
        },
    )?;
    if config.semantic {
        semantic::enrich(&mut graph, &config.project, config.semantic_limit)?;
    }
    if config.mir {
        mir::enrich(&mut graph, &config.project, config.mir_limit)?;
    }
    Ok(graph)
}

pub(crate) fn start_watcher(state: Arc<Mutex<AppState>>, poll: Duration) {
    thread::spawn(move || {
        loop {
            thread::sleep(poll);
            let should_reindex = {
                let state = state.lock().unwrap();
                !state.status.indexing && rust_mtimes(&state.config.project) != state.last_mtimes
            };
            if should_reindex {
                reindex(&state);
            }
        }
    });
}

fn rust_mtimes(root: &Path) -> BTreeMap<PathBuf, SystemTime> {
    let mut result = BTreeMap::new();
    collect_mtimes(root, &mut result);
    result
}

fn collect_mtimes(path: &Path, result: &mut BTreeMap<PathBuf, SystemTime>) {
    let Ok(read_dir) = std::fs::read_dir(path) else {
        return;
    };
    for entry in read_dir.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path
            .components()
            .any(|item| matches!(item.as_os_str().to_str(), Some("target" | ".git")))
        {
            continue;
        }
        if path.is_dir() {
            collect_mtimes(&path, result);
            continue;
        }
        if path.extension().and_then(|item| item.to_str()) == Some("rs")
            || path.file_name().and_then(|item| item.to_str()) == Some("Cargo.toml")
        {
            if let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) {
                result.insert(path, modified);
            }
        }
    }
}

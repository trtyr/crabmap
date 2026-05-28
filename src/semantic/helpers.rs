use crate::model::{CodeGraph, Location, NodeKind, Range};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use url::Url;

pub(crate) fn detect_rust_analyzer() -> Option<String> {
    let command = if cfg!(windows) {
        "rust-analyzer.exe"
    } else {
        "rust-analyzer"
    };
    let candidate = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(command))
            .find(|path| path.is_file())
    })?;
    let status = Command::new(&candidate)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;
    status
        .success()
        .then(|| candidate.to_string_lossy().to_string())
}

pub(crate) fn workspace_root(graph: &CodeGraph, project: &Path) -> anyhow::Result<PathBuf> {
    let project = project.canonicalize()?;
    if graph.project.workspace_root == "." {
        return Ok(project);
    }
    Ok(project.join(&graph.project.workspace_root).canonicalize()?)
}

pub(crate) fn flatten_document_symbols(value: &Value) -> Vec<Value> {
    let mut result = Vec::new();
    for item in value.as_array().into_iter().flatten() {
        result.push(item.clone());
        result.extend(flatten_document_symbols(
            item.get("children").unwrap_or(&Value::Null),
        ));
    }
    result
}

pub(crate) fn lsp_kind(item: &Value) -> NodeKind {
    match item.get("kind").and_then(Value::as_u64).unwrap_or_default() {
        2 | 3 => NodeKind::Module,
        5 => NodeKind::Struct,
        6 => NodeKind::Method,
        7 => NodeKind::Property,
        8 => NodeKind::Field,
        9 => NodeKind::Constructor,
        10 => NodeKind::Enum,
        11 => NodeKind::Trait,
        12 => NodeKind::Function,
        13 => NodeKind::Variable,
        14 => NodeKind::Const,
        22 => NodeKind::EnumMember,
        23 => NodeKind::Struct,
        26 => NodeKind::TypeParameter,
        25 => NodeKind::Function,
        _ => NodeKind::Unknown,
    }
}

pub(crate) fn is_transient_lsp_error(error: &anyhow::Error) -> bool {
    let error = format!("{error:#}");
    error.contains("\"code\":-32801") || error.contains("content modified")
}

pub(crate) fn lsp_range(value: &Value) -> Option<Range> {
    Some(Range {
        start_line: value
            .get("start")?
            .get("line")?
            .as_u64()
            .map(|line| line as usize + 1)?,
        end_line: value
            .get("end")?
            .get("line")?
            .as_u64()
            .map(|line| line as usize + 1)?,
    })
}

pub(crate) fn lsp_location(root: &Path, item: &Value) -> Option<Location> {
    Some(Location {
        file: item
            .get("uri")
            .and_then(Value::as_str)
            .and_then(uri_path)
            .map(|path| relative(root, &path))?,
        line: item
            .get("range")
            .and_then(|range| range.get("start"))
            .and_then(|start| start.get("line"))
            .and_then(Value::as_u64)
            .map(|line| line as usize + 1)
            .unwrap_or(1),
    })
}

pub(crate) fn text_document(file: &Path) -> anyhow::Result<Value> {
    Ok(json!({ "uri": path_uri(file)? }))
}

pub(crate) fn path_uri(path: &Path) -> anyhow::Result<String> {
    Ok(Url::from_file_path(path)
        .map_err(|_| anyhow::anyhow!("failed to convert path to file URI: {}", path.display()))?
        .to_string())
}

pub(crate) fn uri_path(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

pub(crate) fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

pub(crate) fn unique_id(graph: &CodeGraph, base: String) -> String {
    if !graph.nodes.iter().any(|node| node.id == base) {
        return base;
    }
    let mut index = 2;
    loop {
        let candidate = format!("{base}#{index}");
        if !graph.nodes.iter().any(|node| node.id == candidate) {
            return candidate;
        }
        index += 1;
    }
}

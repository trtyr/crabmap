use crate::model::{
    CodeGraph, Edge, EdgeCertainty, EdgeKind, EdgeSource, Location, Node, NodeKind, Range,
    SemanticInfo,
};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use url::Url;

struct LspClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    opened: BTreeSet<PathBuf>,
}

pub fn enrich(graph: &mut CodeGraph, project: &Path, limit: usize) -> Result<()> {
    let root = workspace_root(graph, project)?;
    let Some(command) = detect_rust_analyzer() else {
        graph.warnings.push("semantic enrichment skipped: rust-analyzer not found in PATH".to_string());
        graph.semantic = Some(SemanticInfo {
            provider: "rust-analyzer".to_string(),
            enabled: false,
            scanned_symbols: 0,
            enriched_symbols: 0,
            confirmed_symbols: 0,
            enriched_edges: 0,
            confirmed_edges: 0,
            unresolved_items: 0,
            warnings: vec!["rust-analyzer not found in PATH".to_string()],
        });
        return Ok(());
    };
    let mut client = LspClient::start(&command, &root)?;
    client.initialize(&root)?;
    std::thread::sleep(std::time::Duration::from_millis(750));

    let mut scanned = 0;
    let mut enriched_symbols = 0;
    let mut confirmed_symbols = 0;
    let mut enriched = 0;
    let mut confirmed = 0;
    let mut unresolved = 0;
    let mut warnings = Vec::new();
    for file in graph_files(graph) {
        match enrich_document_symbols(graph, &mut client, &root, &file) {
            Ok(stats) => {
                enriched_symbols += stats.0;
                confirmed_symbols += stats.1;
            }
            Err(error) => warnings.push(format!("{file}: {error:#}")),
        }
    }
    let symbols = graph
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method))
        .filter(|node| node.file.is_some() && node.range.is_some())
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();

    for node in symbols {
        scanned += 1;
        match enrich_symbol(graph, &mut client, &root, &node) {
            Ok(stats) => {
                enriched += stats.0;
                confirmed += stats.1;
                unresolved += stats.2;
            }
            Err(error) if !is_transient_lsp_error(&error) => {
                warnings.push(format!("{}: {error:#}", node.qualified_name))
            }
            Err(_) => {}
        }
    }
    client.shutdown();
    graph.semantic = Some(SemanticInfo {
        provider: "rust-analyzer".to_string(),
        enabled: true,
        scanned_symbols: scanned,
        enriched_symbols,
        confirmed_symbols,
        enriched_edges: enriched,
        confirmed_edges: confirmed,
        unresolved_items: unresolved,
        warnings,
    });
    Ok(())
}

fn graph_files(graph: &CodeGraph) -> Vec<String> {
    graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .map(|node| node.name.clone())
        .collect()
}

fn enrich_document_symbols(
    graph: &mut CodeGraph,
    client: &mut LspClient,
    root: &Path,
    file: &str,
) -> Result<(usize, usize)> {
    let path = root.join(file);
    client.did_open(&path)?;
    let mut result = Value::Null;
    for attempt in 0..3 {
        result = client.request(
            "textDocument/documentSymbol",
            json!({ "textDocument": text_document(&path)? }),
        )?;
        if result.as_array().is_some_and(|items| !items.is_empty()) || attempt == 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let mut enriched = 0;
    let mut confirmed = 0;
    let uri = path_uri(&path)?;
    for mut item in flatten_document_symbols(&result) {
        if let Some(item) = item.as_object_mut() {
            item.entry("uri").or_insert(Value::String(uri.clone()));
        }
        if confirm_or_insert_symbol(graph, root, &item) {
            confirmed += 1;
            continue;
        }
        if resolve_or_insert(graph, root, &item).is_some() {
            enriched += 1;
        }
    }
    Ok((enriched, confirmed))
}

fn flatten_document_symbols(value: &Value) -> Vec<Value> {
    let mut result = Vec::new();
    for item in value.as_array().into_iter().flatten() {
        result.push(item.clone());
        result.extend(flatten_document_symbols(
            item.get("children").unwrap_or(&Value::Null),
        ));
    }
    result
}

fn confirm_or_insert_symbol(graph: &mut CodeGraph, root: &Path, item: &Value) -> bool {
    let Some(name) = item.get("name").and_then(Value::as_str) else {
        return false;
    };
    let file = item
        .get("uri")
        .and_then(Value::as_str)
        .and_then(uri_path)
        .map(|path| relative(root, &path));
    let range = item.get("selectionRange").or_else(|| item.get("range"));
    let line = range
        .and_then(lsp_range)
        .map(|range| range.start_line)
        .unwrap_or_default();
    if let Some(node) = graph.nodes.iter_mut().find(|node| {
        node.name == name
            && file
                .as_ref()
                .is_none_or(|file| node.file.as_ref() == Some(file))
            && node
                .range
                .as_ref()
                .is_none_or(|range| line == 0 || range.start_line == line)
    }) {
        node.metrics
            .insert("rust_analyzer_document_symbol".to_string(), 1);
        return true;
    }
    let candidates = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.name == name
                && file
                    .as_ref()
                    .is_none_or(|file| node.file.as_ref() == Some(file))
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return false;
    }
    if let Some(node) = graph.nodes.get_mut(candidates[0]) {
        node.metrics
            .insert("rust_analyzer_document_symbol".to_string(), 1);
        return true;
    }
    false
}

fn enrich_symbol(
    graph: &mut CodeGraph,
    client: &mut LspClient,
    root: &Path,
    node: &Node,
) -> Result<(usize, usize, usize)> {
    let file = root.join(node.file.as_ref().context("semantic node missing file")?);
    if !file.exists() {
        bail!("source file not found: {}", file.display());
    }
    client.did_open(&file)?;
    let position = symbol_position(&file, node)?;
    let items = client.request(
        "textDocument/prepareCallHierarchy",
        json!({
            "textDocument": text_document(&file)?,
            "position": position
        }),
    )?;
    let Some(item) = items.as_array().and_then(|items| items.first()).cloned() else {
        return Ok((0, 0, 0));
    };
    let outgoing = client.request("callHierarchy/outgoingCalls", json!({ "item": item }))?;
    let mut enriched = 0;
    let mut confirmed = 0;
    let mut unresolved = 0;
    for call in outgoing.as_array().into_iter().flatten() {
        let Some(target) = call.get("to") else {
            unresolved += 1;
            continue;
        };
        let target_id = resolve_or_insert(graph, root, target);
        if let Some(target_id) = target_id {
            match add_edge(
                graph,
                &node.id,
                &target_id,
                EdgeKind::Calls,
                Some("rust-analyzer outgoing call".to_string()),
                lsp_location(root, target),
            ) {
                EdgeResult::Inserted => enriched += 1,
                EdgeResult::Confirmed => confirmed += 1,
                EdgeResult::Skipped => {}
            }
            continue;
        }
        unresolved += 1;
    }
    Ok((enriched, confirmed, unresolved))
}

fn resolve_or_insert(graph: &mut CodeGraph, root: &Path, item: &Value) -> Option<String> {
    let name = item.get("name").and_then(Value::as_str)?.to_string();
    let absolute_file = item.get("uri").and_then(Value::as_str).and_then(uri_path);
    if absolute_file
        .as_ref()
        .is_some_and(|path| !path.starts_with(root))
    {
        return None;
    }
    let file = absolute_file.map(|path| relative(root, &path));
    if let Some(existing) = graph.nodes.iter().find(|node| {
        node.name == name
            && file
                .as_ref()
                .is_none_or(|file| node.file.as_ref() == Some(file))
    }) {
        return Some(existing.id.clone());
    }
    let qualified_name = item
        .get("detail")
        .and_then(Value::as_str)
        .filter(|detail| !detail.trim().is_empty())
        .map(|detail| format!("{detail}::{name}"))
        .unwrap_or_else(|| {
            file.as_ref()
                .map(|file| format!("{file}::{name}"))
                .unwrap_or_else(|| name.clone())
        });
    let id = unique_id(
        graph,
        format!("{}:{qualified_name}", lsp_kind(item).as_str()).replace([' ', '\n', '\t'], "_"),
    );
    graph.nodes.push(Node {
        id: id.clone(),
        kind: lsp_kind(item),
        name,
        qualified_name,
        file,
        range: item.get("range").and_then(lsp_range),
        visibility: None,
        signature: item
            .get("detail")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        docs: None,
        metrics: BTreeMap::new(),
    });
    Some(id)
}

fn add_edge(
    graph: &mut CodeGraph,
    from: &str,
    to: &str,
    kind: EdgeKind,
    label: Option<String>,
    evidence: Option<Location>,
) -> EdgeResult {
    if from == to {
        return EdgeResult::Skipped;
    }
    if let Some(edge) = graph
        .edges
        .iter_mut()
        .find(|edge| edge.from == from && edge.to == to && edge.kind == kind)
    {
        edge.weight += 1;
        edge.source = EdgeSource::RustAnalyzer;
        edge.certainty = EdgeCertainty::Confirmed;
        return EdgeResult::Confirmed;
    }
    let profiles = graph
        .profiles
        .iter()
        .map(|profile| profile.name.clone())
        .collect();
    graph.edges.push(Edge {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        label,
        evidence,
        weight: 1,
        source: EdgeSource::RustAnalyzer,
        certainty: EdgeCertainty::Confirmed,
        profiles,
    });
    EdgeResult::Inserted
}

enum EdgeResult {
    Inserted,
    Confirmed,
    Skipped,
}

fn workspace_root(graph: &CodeGraph, project: &Path) -> Result<PathBuf> {
    let project = project.canonicalize()?;
    if graph.project.workspace_root == "." {
        return Ok(project);
    }
    Ok(project.join(&graph.project.workspace_root).canonicalize()?)
}

fn symbol_position(file: &Path, node: &Node) -> Result<Value> {
    let source = std::fs::read_to_string(file)?;
    let line = node
        .range
        .as_ref()
        .map(|range| range.start_line)
        .unwrap_or(1)
        .saturating_sub(1);
    let character = source
        .lines()
        .nth(line)
        .and_then(|text| text.find(&node.name))
        .unwrap_or_default();
    Ok(json!({ "line": line, "character": character }))
}

fn lsp_kind(item: &Value) -> NodeKind {
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

fn is_transient_lsp_error(error: &anyhow::Error) -> bool {
    let error = format!("{error:#}");
    error.contains("\"code\":-32801") || error.contains("content modified")
}

fn lsp_range(value: &Value) -> Option<Range> {
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

fn lsp_location(root: &Path, item: &Value) -> Option<Location> {
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

fn text_document(file: &Path) -> Result<Value> {
    Ok(json!({ "uri": path_uri(file)? }))
}

fn path_uri(path: &Path) -> Result<String> {
    Ok(Url::from_file_path(path)
        .map_err(|_| anyhow::anyhow!("failed to convert path to file URI: {}", path.display()))?
        .to_string())
}

fn uri_path(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn unique_id(graph: &CodeGraph, base: String) -> String {
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

impl LspClient {
    fn start(command: &str, root: &Path) -> Result<Self> {
        let mut child = Command::new(command)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to start `{command}`"))?;
        Ok(Self {
            stdin: child.stdin.take().context("LSP server stdin unavailable")?,
            stdout: BufReader::new(
                child
                    .stdout
                    .take()
                    .context("LSP server stdout unavailable")?,
            ),
            child,
            next_id: 1,
            opened: BTreeSet::new(),
        })
    }

    fn initialize(&mut self, root: &Path) -> Result<()> {
        let root_uri = path_uri(root)?;
        self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "workspaceFolders": [{ "name": "workspace", "uri": root_uri }],
                "capabilities": {
                    "window": { "workDoneProgress": true },
                    "workspace": {
                        "configuration": true,
                        "workspaceFolders": true,
                        "symbol": { "dynamicRegistration": false }
                    },
                    "textDocument": {
                        "synchronization": { "didOpen": true },
                        "documentSymbol": { "dynamicRegistration": false, "hierarchicalDocumentSymbolSupport": true },
                        "definition": { "dynamicRegistration": false, "linkSupport": true },
                        "references": { "dynamicRegistration": false },
                        "implementation": { "dynamicRegistration": false, "linkSupport": true },
                        "callHierarchy": { "dynamicRegistration": false }
                    }
                }
            }),
        )?;
        self.notify("initialized", json!({}))?;
        self.notify(
            "workspace/didChangeConfiguration",
            json!({ "settings": {} }),
        )?;
        Ok(())
    }

    fn did_open(&mut self, file: &Path) -> Result<()> {
        let file = file.canonicalize()?;
        if !self.opened.insert(file.clone()) {
            return Ok(());
        }
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": path_uri(&file)?,
                    "languageId": "rust",
                    "version": 0,
                    "text": std::fs::read_to_string(file)?
                }
            }),
        )
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        loop {
            let message = self.read_message()?;
            if message.get("method").is_some() && message.get("id").is_some() {
                self.respond_to_server_request(&message)?;
                continue;
            }
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                bail!("LSP request `{method}` failed: {error}");
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    fn shutdown(&mut self) {
        let _ = self.request("shutdown", Value::Null);
        let _ = self.notify("exit", Value::Null);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn respond_to_server_request(&mut self, message: &Value) -> Result<()> {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": message.get("id").cloned().unwrap_or(Value::Null),
            "result": match message.get("method").and_then(Value::as_str).unwrap_or("") {
                "workspace/configuration" => json!([{}]),
                "workspace/workspaceFolders" => json!([]),
                "window/workDoneProgress/create" => Value::Null,
                "client/registerCapability" | "client/unregisterCapability" => Value::Null,
                _ => Value::Null,
            }
        }))
    }

    fn send(&mut self, message: Value) -> Result<()> {
        let body = serde_json::to_vec(&message)?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len())?;
        self.stdin.write_all(&body)?;
        self.stdin.flush()?;
        Ok(())
    }

    fn read_message(&mut self) -> Result<Value> {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            let bytes = self.stdout.read_line(&mut line)?;
            if bytes == 0 {
                bail!("LSP server exited before responding");
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = trimmed.strip_prefix("Content-Length:") {
                content_length = Some(value.trim().parse::<usize>()?);
            }
        }
        let len = content_length.context("LSP response missing Content-Length")?;
        let mut body = vec![0; len];
        self.stdout.read_exact(&mut body)?;
        Ok(serde_json::from_slice(&body)?)
    }
}

fn detect_rust_analyzer() -> Option<String> {
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
    status.success().then(|| candidate.to_string_lossy().to_string())
}

use crate::analyzer::{self, IndexOptions};
use crate::cli::ServeArgs;
use crate::mir;
use crate::model::CodeGraph;
use crate::query;
use crate::semantic;
use crate::store;
use crate::term;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

const INDEX_HTML: &str = include_str!("../web/index.html");
const BASE_CSS: &str = include_str!("../web/styles/base.css");
const LAYOUT_CSS: &str = include_str!("../web/styles/layout.css");
const COMPONENTS_CSS: &str = include_str!("../web/styles/components.css");
const GRAPH_CSS: &str = include_str!("../web/styles/graph.css");
const CORE_JS: &str = include_str!("../web/src/core.js");
const UTILS_JS: &str = include_str!("../web/src/utils.js");
const API_JS: &str = include_str!("../web/src/api.js");
const GRAPH_LAYOUT_JS: &str = include_str!("../web/src/graph-layout.js");
const GRAPH_RENDER_JS: &str = include_str!("../web/src/graph-render.js");
const GRAPH_INTERACT_JS: &str = include_str!("../web/src/graph-interact.js");
const SIDEBAR_JS: &str = include_str!("../web/src/sidebar.js");
const DETAILS_JS: &str = include_str!("../web/src/details.js");
const TOOLBAR_JS: &str = include_str!("../web/src/toolbar.js");
const MAIN_JS: &str = include_str!("../web/src/main.js");

#[derive(Clone)]
struct ServeConfig {
    project: PathBuf,
    host: String,
    port: u16,
    graph: Option<PathBuf>,
    watch_seconds: Option<Duration>,
    include_tests: bool,
    features: Vec<String>,
    all_features: bool,
    no_default_features: bool,
    target: Option<String>,
    profile: String,
    semantic: bool,
    semantic_limit: usize,
    mir: bool,
    mir_limit: usize,
}

struct AppState {
    config: ServeConfig,
    graph: Option<CodeGraph>,
    graph_gz: Option<Vec<u8>>,
    status: Status,
    last_mtimes: BTreeMap<PathBuf, SystemTime>,
}

#[derive(Clone, Serialize)]
struct Status {
    project: String,
    graph_path: String,
    indexing: bool,
    last_event: String,
    last_index_ms: u128,
    errors: Vec<String>,
}

pub fn serve(args: ServeArgs) -> Result<()> {
    let config = ServeConfig::from(args);
    let state = Arc::new(Mutex::new(AppState {
        status: Status {
            project: config.project.display().to_string(),
            graph_path: store::default_path(config.graph.as_deref())?
                .display()
                .to_string(),
            indexing: false,
            last_event: "starting".to_string(),
            last_index_ms: 0,
            errors: Vec::new(),
        },
        last_mtimes: BTreeMap::new(),
        graph: None,
        graph_gz: None,
        config: config.clone(),
    }));
    reindex(&state);
    if config.graph.is_some() {
        let _ = store_current_graph(&state);
    }
    if config.watch_seconds().is_some() {
        start_watcher(state.clone(), config.watch_seconds().unwrap());
    }

    let listener = TcpListener::bind(format!("{}:{}", config.host(), config.port()))
        .with_context(|| format!("failed to bind {}:{}", config.host(), config.port()))?;
    println!(
        "{} {}",
        term::green("ferrimind viewer"),
        term::cyan(&format!("http://{}:{}", config.host(), config.port()))
    );
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = state.clone();
                thread::spawn(move || {
                    let _ = handle(stream, &state);
                });
            }
            Err(error) => eprintln!("{error:#}"),
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream, state: &Arc<Mutex<AppState>>) -> Result<()> {
    let mut buffer = [0; 8192];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let line = request.lines().next().unwrap_or_default();
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    if method != "GET" && method != "POST" {
        return respond(&mut stream, 405, "text/plain", b"method not allowed");
    }
    let (path, query_string) = target.split_once('?').unwrap_or((target, ""));
    let query = parse_query(query_string);
    match path {
        "/" => respond(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes(),
        ),
        "/api/status" => json_response(&mut stream, status(state)),
        "/api/graph" => {
            let gz = state.lock().unwrap().graph_gz.clone();
            if let Some(ref gz) = gz {
                return respond_gz(&mut stream, gz);
            }
            json_response(&mut stream, graph_payload(state))
        }
        "/api/search" => json_response(
            &mut stream,
            with_graph(state, |graph| {
                query::search(
                    graph,
                    query.get("q").map(String::as_str).unwrap_or_default(),
                    query
                        .get("limit")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(50),
                )
            }),
        ),
        "/api/symbol" => json_response(
            &mut stream,
            with_graph_result(state, |graph| {
                query::symbol(
                    graph,
                    query.get("name").map(String::as_str).unwrap_or_default(),
                )
            }),
        ),
        "/api/callees" => walk_response(&mut stream, state, &query, true),
        "/api/callers" => walk_response(&mut stream, state, &query, false),
        "/api/impact" => json_response(
            &mut stream,
            with_graph_result(state, |graph| {
                query::impact(
                    graph,
                    query.get("name").map(String::as_str).unwrap_or_default(),
                    query
                        .get("depth")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(2),
                    query
                        .get("limit")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(100),
                )
            }),
        ),
        "/styles/base.css" => respond(&mut stream, 200, "text/css; charset=utf-8", BASE_CSS.as_bytes()),
        "/styles/layout.css" => respond(&mut stream, 200, "text/css; charset=utf-8", LAYOUT_CSS.as_bytes()),
        "/styles/components.css" => respond(&mut stream, 200, "text/css; charset=utf-8", COMPONENTS_CSS.as_bytes()),
        "/styles/graph.css" => respond(&mut stream, 200, "text/css; charset=utf-8", GRAPH_CSS.as_bytes()),
        "/src/core.js" => respond(&mut stream, 200, "application/javascript; charset=utf-8", CORE_JS.as_bytes()),
        "/src/utils.js" => respond(&mut stream, 200, "application/javascript; charset=utf-8", UTILS_JS.as_bytes()),
        "/src/api.js" => respond(&mut stream, 200, "application/javascript; charset=utf-8", API_JS.as_bytes()),
        "/src/graph-layout.js" => respond(&mut stream, 200, "application/javascript; charset=utf-8", GRAPH_LAYOUT_JS.as_bytes()),
        "/src/graph-render.js" => respond(&mut stream, 200, "application/javascript; charset=utf-8", GRAPH_RENDER_JS.as_bytes()),
        "/src/graph-interact.js" => respond(&mut stream, 200, "application/javascript; charset=utf-8", GRAPH_INTERACT_JS.as_bytes()),
        "/src/sidebar.js" => respond(&mut stream, 200, "application/javascript; charset=utf-8", SIDEBAR_JS.as_bytes()),
        "/src/details.js" => respond(&mut stream, 200, "application/javascript; charset=utf-8", DETAILS_JS.as_bytes()),
        "/src/toolbar.js" => respond(&mut stream, 200, "application/javascript; charset=utf-8", TOOLBAR_JS.as_bytes()),
        "/src/main.js" => respond(&mut stream, 200, "application/javascript; charset=utf-8", MAIN_JS.as_bytes()),
        "/api/reindex" => {
            let state = state.clone();
            thread::spawn(move || reindex(&state));
            json_response(&mut stream, json!({ "ok": true }))
        }
        _ => respond(&mut stream, 404, "text/plain", b"not found"),
    }
}

fn walk_response(
    stream: &mut TcpStream,
    state: &Arc<Mutex<AppState>>,
    query: &BTreeMap<String, String>,
    outbound: bool,
) -> Result<()> {
    json_response(
        stream,
        with_graph_result(state, |graph| {
            query::neighbors(
                graph,
                query.get("name").map(String::as_str).unwrap_or_default(),
                "calls",
                outbound,
                query
                    .get("depth")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(2),
                query
                    .get("limit")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(100),
            )
        }),
    )
}

fn reindex(state: &Arc<Mutex<AppState>>) {
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
                use flate2::write::GzEncoder;
                use flate2::Compression;
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

fn start_watcher(state: Arc<Mutex<AppState>>, poll: Duration) {
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

fn graph_payload(state: &Arc<Mutex<AppState>>) -> Value {
    with_graph(state, |graph| {
        json!({
            "kind": "graph",
            "stats": graph.stats(),
            "nodes": &graph.nodes,
            "edges": &graph.edges,
            "profiles": &graph.profiles,
            "warnings": &graph.warnings
        })
    })
}

fn status(state: &Arc<Mutex<AppState>>) -> Value {
    json!(state.lock().unwrap().status.clone())
}

fn with_graph(state: &Arc<Mutex<AppState>>, callback: impl FnOnce(&CodeGraph) -> Value) -> Value {
    let state = state.lock().unwrap();
    state
        .graph
        .as_ref()
        .map(callback)
        .unwrap_or_else(|| json!({ "error": "graph not ready" }))
}

fn with_graph_result(
    state: &Arc<Mutex<AppState>>,
    callback: impl FnOnce(&CodeGraph) -> Result<Value>,
) -> Value {
    let state = state.lock().unwrap();
    match state.graph.as_ref().map(callback) {
        Some(Ok(value)) => value,
        Some(Err(error)) => json!({ "error": format!("{error:#}") }),
        None => json!({ "error": "graph not ready" }),
    }
}

fn store_current_graph(state: &Arc<Mutex<AppState>>) -> Result<()> {
    store_current_graph_locked(&state.lock().unwrap())
}

fn store_current_graph_locked(state: &AppState) -> Result<()> {
    if let Some(graph) = &state.graph {
        store::save(state.config.graph.as_deref(), graph)?;
    }
    Ok(())
}

fn json_response(stream: &mut TcpStream, value: Value) -> Result<()> {
    respond(
        stream,
        200,
        "application/json; charset=utf-8",
        &serde_json::to_vec(&value)?,
    )
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}

fn respond_gz(stream: &mut TcpStream, body: &[u8]) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}

fn parse_query(query: &str) -> BTreeMap<String, String> {
    query
        .split('&')
        .filter_map(|part| part.split_once('='))
        .map(|(key, value)| (decode(key), decode(value)))
        .collect()
}

fn decode(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.as_bytes().iter().copied();
    while let Some(char) = chars.next() {
        if char == b'+' {
            result.push(' ');
            continue;
        }
        if char == b'%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(hi), Some(lo)) = (hi, lo) {
                if let Ok(byte) = u8::from_str_radix(&format!("{}{}", hi as char, lo as char), 16) {
                    result.push(byte as char);
                    continue;
                }
            }
        }
        result.push(char as char);
    }
    result
}

impl ServeConfig {
    fn host(&self) -> &str {
        &self.host
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn watch_seconds(&self) -> Option<Duration> {
        self.watch_seconds
    }
}

impl From<ServeArgs> for ServeConfig {
    fn from(args: ServeArgs) -> Self {
        Self {
            project: args.project,
            host: args.host,
            port: args.port,
            graph: args.graph,
            watch_seconds: args.watch.then(|| Duration::from_secs(2)),
            include_tests: !args.index.no_tests,
            features: args.index.features,
            all_features: args.index.all_features,
            no_default_features: args.index.no_default_features,
            target: args.index.target,
            profile: "default".to_string(),
            semantic: !args.index.no_semantic,
            semantic_limit: args.index.semantic_limit,
            mir: args.index.mir,
            mir_limit: args.index.mir_limit,
        }
    }
}

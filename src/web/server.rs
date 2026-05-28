use super::assets::*;
use super::config::ServeConfig;
use super::helpers::{
    graph_payload, json_response, parse_query, respond, respond_gz, status, store_current_graph,
    with_graph, with_graph_result,
};
use super::indexing::{reindex, start_watcher};
use super::state::{AppState, Status};
use crate::cli::ServeArgs;
use crate::query;
use crate::store;
use crate::term;
use anyhow::{Context, Result};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

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
        term::green("crabmap viewer"),
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
        "/styles/base.css" => respond(
            &mut stream,
            200,
            "text/css; charset=utf-8",
            BASE_CSS.as_bytes(),
        ),
        "/styles/layout.css" => respond(
            &mut stream,
            200,
            "text/css; charset=utf-8",
            LAYOUT_CSS.as_bytes(),
        ),
        "/styles/components.css" => respond(
            &mut stream,
            200,
            "text/css; charset=utf-8",
            COMPONENTS_CSS.as_bytes(),
        ),
        "/styles/graph.css" => respond(
            &mut stream,
            200,
            "text/css; charset=utf-8",
            GRAPH_CSS.as_bytes(),
        ),
        "/src/core.js" => respond(
            &mut stream,
            200,
            "application/javascript; charset=utf-8",
            CORE_JS.as_bytes(),
        ),
        "/src/utils.js" => respond(
            &mut stream,
            200,
            "application/javascript; charset=utf-8",
            UTILS_JS.as_bytes(),
        ),
        "/src/api.js" => respond(
            &mut stream,
            200,
            "application/javascript; charset=utf-8",
            API_JS.as_bytes(),
        ),
        "/src/graph-layout.js" => respond(
            &mut stream,
            200,
            "application/javascript; charset=utf-8",
            GRAPH_LAYOUT_JS.as_bytes(),
        ),
        "/src/graph-render.js" => respond(
            &mut stream,
            200,
            "application/javascript; charset=utf-8",
            GRAPH_RENDER_JS.as_bytes(),
        ),
        "/src/graph-interact.js" => respond(
            &mut stream,
            200,
            "application/javascript; charset=utf-8",
            GRAPH_INTERACT_JS.as_bytes(),
        ),
        "/src/sidebar.js" => respond(
            &mut stream,
            200,
            "application/javascript; charset=utf-8",
            SIDEBAR_JS.as_bytes(),
        ),
        "/src/details.js" => respond(
            &mut stream,
            200,
            "application/javascript; charset=utf-8",
            DETAILS_JS.as_bytes(),
        ),
        "/src/toolbar.js" => respond(
            &mut stream,
            200,
            "application/javascript; charset=utf-8",
            TOOLBAR_JS.as_bytes(),
        ),
        "/src/main.js" => respond(
            &mut stream,
            200,
            "application/javascript; charset=utf-8",
            MAIN_JS.as_bytes(),
        ),
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

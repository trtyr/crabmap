use super::state::AppState;
use crate::model::CodeGraph;
use crate::store;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

pub(crate) fn graph_payload(state: &Arc<Mutex<AppState>>) -> Value {
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

pub(crate) fn status(state: &Arc<Mutex<AppState>>) -> Value {
    json!(state.lock().unwrap().status.clone())
}

pub(crate) fn with_graph(
    state: &Arc<Mutex<AppState>>,
    callback: impl FnOnce(&CodeGraph) -> Value,
) -> Value {
    let state = state.lock().unwrap();
    state
        .graph
        .as_ref()
        .map(callback)
        .unwrap_or_else(|| json!({ "error": "graph not ready" }))
}

pub(crate) fn with_graph_result(
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

pub(crate) fn store_current_graph(state: &Arc<Mutex<AppState>>) -> Result<()> {
    store_current_graph_locked(&state.lock().unwrap())
}

pub(crate) fn store_current_graph_locked(state: &AppState) -> Result<()> {
    if let Some(graph) = &state.graph {
        store::save(state.config.graph.as_deref(), graph)?;
    }
    Ok(())
}

pub(crate) fn json_response(stream: &mut TcpStream, value: Value) -> Result<()> {
    respond(
        stream,
        200,
        "application/json; charset=utf-8",
        &serde_json::to_vec(&value)?,
    )
}

pub(crate) fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
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

pub(crate) fn respond_gz(stream: &mut TcpStream, body: &[u8]) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}

pub(crate) fn parse_query(query: &str) -> BTreeMap<String, String> {
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

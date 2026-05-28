use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use super::helpers::path_uri;

pub(crate) struct LspClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    opened: BTreeSet<PathBuf>,
}

impl LspClient {
    pub(crate) fn start(command: &str, root: &Path) -> Result<Self> {
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

    pub(crate) fn initialize(&mut self, root: &Path) -> Result<()> {
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

    pub(crate) fn did_open(&mut self, file: &Path) -> Result<()> {
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

    pub(crate) fn request(&mut self, method: &str, params: Value) -> Result<Value> {
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

    pub(crate) fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    pub(crate) fn shutdown(&mut self) {
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

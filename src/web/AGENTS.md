# src/web/ — HTTP Server Backend

**Generated:** 2026-05-29 · **Commit:** b9dae4c

Rust-side HTTP server that serves the embedded `web/` frontend and provides the JSON API. Distinct from `web/` (frontend) — this is the backend. 7 files, 628 lines.

## Structure

```
src/web/
├── mod.rs          # Re-exports serve()
├── server.rs       # Raw TCP server: bind, accept loop, route dispatch
├── state.rs        # AppState + Status, Arc<Mutex<AppState>> for thread safety
├── config.rs       # ServeConfig (from cli::ServeArgs)
├── indexing.rs     # reindex() + start_watcher() — file watch & rebuild
├── helpers.rs      # load_or_index(), respond(), respond_json(), gzip helpers
└── assets.rs       # 15× include_str! constants for web/ frontend files
```

## Where to Look

| Task | File | Notes |
|---|---|---|
| Add API endpoint | `server.rs` route match → `helpers.rs` responder | Route dispatch is a manual match on URL path |
| Change state model | `state.rs::AppState` | All handlers access via `Arc<Mutex<AppState>>` |
| Add server config | `config.rs::ServeConfig` | Mirrors `cli::ServeArgs` fields |
| Fix reindexing | `indexing.rs` | Thread-spawned, polling-based file watch |
| Add frontend asset | `assets.rs` + `web/` file | `include_str!` embeds at compile time |
| Fix response format | `helpers.rs::respond_json()` | JSON serialization + CORS headers |

## Architecture

### Server Lifecycle

```
serve(config)
  ├── load_or_index() → CodeGraph, gzip it once
  ├── AppState { graph, graph_gz, status, config }
  ├── start_watcher() if --watch (optional thread)
  └── TcpListener bind → accept loop
      └── thread::spawn(handle(stream, &state))
```

### State Management

```rust
pub(crate) struct AppState {
    pub graph: Option<Arc<CodeGraph>>,
    pub graph_gz: Option<Vec<u8>>,    // Pre-gzipped for fast /api/graph
    pub status: Status,               // idle | indexing | ready
    pub config: ServeConfig,
    pub graph_path: Option<PathBuf>,
}
```

- **Thread safety**: `Arc<Mutex<AppState>>` shared across all request threads.
- **Access helpers**: `helpers.rs` provides `with_graph()` and `with_graph_result()` closures that lock/unlock the mutex for scoped read access.
- **Pre-gzipped**: `graph_gz` is computed once at startup — every `/api/graph` response serves this cached blob.

### Route Dispatch (server.rs)

Route matching is a manual `if/else` chain on the URL path — no router library:

| Method | Path | Handler |
|---|---|---|
| `GET` | `/` | `INDEX_HTML` (from assets.rs) |
| `GET` | `/styles/*.css` | Corresponding `include_str!` constant |
| `GET` | `/src/*.js` | Corresponding `include_str!` constant |
| `GET` | `/api/status` | `status_json()` → JSON |
| `GET` | `/api/graph` | `graph_payload()` → gzip JSON |
| `GET` | `/api/search?q=&limit=` | Query search → JSON |
| `GET` | `/api/symbol?id=` | Query symbol → JSON |
| `GET` | `/api/callees?id=&depth=` | Query callees → JSON |
| `GET` | `/api/callers?id=&depth=` | Query callers → JSON |
| `GET` | `/api/impact?id=&depth=` | Query impact → JSON |
| `POST` | `/api/reindex` | Triggers `reindex()` → `{"ok": true}` |
| `*` | any other | `404 Not Found` |

## Conventions

- **Synchronous only**: raw TCP server, no tokio, no async. Each request spawns a `thread::spawn`.
- **Blocking IO**: `reqwest::blocking::Client` is the only HTTP client used. All file IO is synchronous.
- **`pub(crate)` everything**: only `serve()` is bare `pub`. All internal types are `pub(crate)`.
- **Status polling**: frontend toolbar polls `/api/status` every 1.5s. Status is a 3-state enum: `idling`, `indexing`, `ready`.
- **CORS headers**: all API responses include `Access-Control-Allow-Origin: *` — server is local-only by intent.
- **Gzip transport**: `/api/graph` responds with pre-compressed gzip + `Content-Encoding: gzip` header.

### Asset Embedding (assets.rs)

```rust
pub(crate) const INDEX_HTML: &str = include_str!("../../web/index.html");
pub(crate) const BASE_CSS: &str    = include_str!("../../web/styles/base.css");
// ... 15 total
```

- **No dynamic file serving** — all assets baked into binary at compile time.
- **Every frontend change requires `cargo build`** to take effect.
- Adding a 16th `include_str!` increases binary size and compile time. Consider abstracting to a macro or dynamic `include_dir!` for new assets.

### File Watching (indexing.rs)

- `start_watcher(project, state, poll_secs)` spawns a polling thread.
- **Polling-based**: checks file mtimes every N seconds (not inotify/fsevent).
- Auto-detects project changes and triggers `reindex()`.
- `reindex()` runs `index_project()` in a spawned thread, updates `AppState` on completion.
- **No graceful shutdown**: watcher thread has no stop signal. Dies with process.

## Anti-Patterns (This Module)

- **`thread::spawn` with no bound**: each HTTP request spawns an unbounded thread — no thread pool, no limit, no join handle. High concurrency will exhaust OS threads.
- **`unwrap()` on Mutex locks**: `.lock().unwrap()` in 7+ locations panics on poisoned mutex. Use `lock().map_err()` or `lock().ok()` with error handling.
- **Swallowed errors in accept loop**: `let _ = handle(stream, &state)` drops all connection errors silently. At minimum, log the error.
- **Manual URL decoding**: `helpers.rs` implements custom `%XX` decoder instead of using the `url` crate's `percent_encoding`. Edge cases may differ from RFC 3986.
- **Blocking HTTP in request threads**: `reindex()` calls `index_project()` which may call `reqwest::blocking` (via semantic enrichment) — blocks the watcher thread.

## Key Types

| Type | Defined In | Purpose |
|---|---|---|
| `AppState` | `state.rs` | Central state: graph, gzip cache, status, config |
| `Status` | `state.rs` | Enum: `Idling`, `Indexing`, `Ready` |
| `ServeConfig` | `config.rs` | Port, host, project path, watch interval, index flags |

## Gotchas

- The server binds `127.0.0.1` by default — not accessible from other machines.
- `graph_gz` is computed once at startup. If `reindex()` rebuilds the graph, `graph_gz` is recomputed.
- Route matching order: `/api/status` must be checked before `/api/s`-prefixed paths to avoid mismatch.
- The server has no graceful shutdown — Ctrl+C kills all spawned threads immediately.
- Watch mode polling interval is default 2 seconds; too frequent polling wastes CPU, too infrequent feels sluggish.

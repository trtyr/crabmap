# ferrimind

Rust code knowledge graph for AI navigation. Single crate, no workspace.

## Build & Run

```bash
cargo build                # debug
cargo build --release      # optimized (recommended) — enables build.rs git-info + progress bar
cargo run -- <subcommand>  # e.g. cargo run -- query search "load config"
cargo test                 # 6 unit + 8 integration tests
```

Edition 2024 — requires a recent stable Rust toolchain (≥ 1.85).

`build.rs` captures git commit and build date at compile time for `--version`. Falls back to `no-git` outside a repo.

## Architecture

Single binary, all Rust modules in `src/`:

| Module | Purpose |
|---|---|
| `main.rs` | CLI entry, command dispatch, `index_project()` helper |
| `cli.rs` | clap definitions: 6 top-level commands (`index`, `serve`, `query`, `nav`, `analyze`, `config`), each with nested subcommand enums |
| `model.rs` | CodeGraph, Node, Edge, NodeKind, EdgeKind — core data model |
| `analyzer.rs` | AST indexer: cargo metadata → syn walk → graph construction. Same-module priority for function/method resolution |
| `store.rs` | Graph JSON load/save, default path resolution (`.ferrimind/ferrimind.json`) |
| `query.rs` | Adjacency index + traversal (search, callers, callees, impact, path, file, module, symbol). `find_nodes()` with priority: exact id/qualified_name > short name > suffix |
| `semantic.rs` | rust-analyzer LSP enrichment, auto-detected on PATH (opt-out via `--no-semantic`) |
| `mir.rs` | rustc MIR text parsing for lowered calls (`--mir` flag) |
| `ai.rs` | AI navigation commands (guide, entries, clusters, quality, health, map) |
| `rag.rs` | Retrieval: lexical search → embedding similarity → reranking |
| `llm.rs` | LLM client for `ask` command |
| `config.rs` | Global LLM/RAG config read/write (`~/.config/ferrimind/config.json`) |
| `report.rs` | GRAPH_REPORT.md and AGENT_GUIDE.md generation |
| `health.rs` | Architectural risk detection (cycles, god modules, dead public symbols) |
| `deps.rs` | Module dependency direction analysis |
| `test_impact.rs` | Static test candidate discovery |
| `gitintel.rs` | Git churn/ownership/co-change analysis (requires git repo) |
| `drift.rs` | Graph diff against git base |
| `repo_map.rs` | Token-budgeted repository map |
| `export.rs` | DOT/Mermaid/JSON export |
| `term.rs` | ANSI terminal colors (red, green, yellow, cyan, bold) with TTY detection |
| `web.rs` | Embedded HTTP viewer: serves static web assets via `include_str!` |

### Web UI (`web/`)

Dark theme, microkernel architecture. 15 modular files served via `include_str!`:

| File | Purpose |
|---|---|
| `index.html` | HTML skeleton, sidebar (search, edge filter pills, metrics), canvas area (graph SVG, edge legend, status bar, zoom controls, detail drawer) |
| `styles/base.css` | CSS variables, reset, typography |
| `styles/layout.css` | Grid, panels, sidebar (260px), drawer |
| `styles/components.css` | Buttons, cards, pills, inputs, edge filter pills |
| `styles/graph.css` | SVG nodes/edges, edge labels, edge legend |
| `src/core.js` | Microkernel: state store + event bus |
| `src/utils.js` | Helpers, `nodeColor()`, `edgeColor()`, `edgeLegend()` |
| `src/api.js` | HTTP client (`/api/status`, `/api/graph`, `/api/search`, `/api/symbol`, `/api/callees`, `/api/callers`, `/api/impact`, `/api/reindex`) |
| `src/graph-layout.js` | Seeded positions + force-directed relaxation |
| `src/graph-render.js` | SVG rendering: per-kind colored edges, trimmed line endpoints, arrow markers, per-kind edge legend, node circles with degree-based radius |
| `src/graph-interact.js` | Drag/zoom/select |
| `src/sidebar.js` | Search results, edge filter pills (Chinese labels + colored dots, localStorage), metrics |
| `src/details.js` | Detail drawer + file symbol listing |
| `src/toolbar.js` | Search, depth, reindex, status, auto-select `ferrimind::run` |
| `src/main.js` | Bootstrap |

## CLI Structure

```
ferrimind
├── index [PROJECT]        # Build graph (--all for workspace, --no-tests, --no-semantic, --mir)
├── serve [PROJECT]        # Start HTTP viewer + API (--port, --watch)
├── query                  # Read operations
│   ├── stats              #   Node/edge counts by kind, source, certainty
│   ├── summary            #   Hot symbols ranked by degree
│   ├── symbols            #   All symbols (--kind, --limit)
│   ├── symbol <NAME>      #   Single symbol (ambiguous if multiple matches)
│   ├── file <PATH>        #   Symbols declared in a file
│   ├── module <NAME>      #   Symbols declared in a module
│   ├── callees <ID>       #   Downstream call graph (--depth)
│   ├── callers <ID>       #   Upstream call graph (--depth)
│   ├── impact <ID>        #   Full dependency impact (--depth)
│   ├── search <QUERY>     #   Text search across names/signatures/docs
│   ├── path <FROM> <TO>   #   Shortest path between two symbols
│   └── export             #   DOT/Mermaid/JSON export (--format)
├── nav                     # AI-oriented navigation
│   ├── guide              #   Entry points + callee chains
│   ├── entries            #   Detected entry points
│   ├── clusters           #   Feature clusters by file
│   ├── quality            #   Graph quality score + recommendations
│   ├── health             #   Cycles, god modules, dead code
│   └── map                #   Token-budgeted repository overview
├── analyze                 # Static analysis (some require git)
│   ├── deps               #   Module dependency matrix
│   ├── fanout             #   File-level fan-in/fan-out
│   ├── tests              #   Test impact candidates
│   ├── hotspots           #   Git churn hotspots
│   └── diff               #   Graph diff vs git base
└── config                  # LLM/RAG API keys and model settings
```

## Key Design Decisions

### Symbol resolution
- `find_nodes()` priority: exact id match > exact qualified_name > short name match > suffix match.
- Ambiguous short names return all matches; traversal commands (callees/callers/impact/path) error with "ambiguous" listing all qualified names.
- "Not found" errors include fuzzy suggestions (Levenshtein distance ≤ 3 closest matches).

### Progress reporting
- Indexing collects all files first, then processes with a progress bar (via `indicatif`).
- Progress bar outputs to stderr, auto-hidden when stderr is not a TTY.
- Completion summary line printed to stderr: ✓ indexed N nodes, M edges in F files.

### Terminal output
- Colored terminal output via ANSI codes (`term` module): red for errors, green for success, yellow for warnings, cyan for URLs.
- Color auto-disabled when stderr is piped (checked via `IsTerminal`).

### Error messages
- Symbol/file/module "not found" errors include Levenshtein-based suggestions.
- Format: `symbol 'inde_project' not found\nDid you mean?\n  • index_project`

### Edge coloring (Web UI)
- Edges colored by `kind`, not source: `calls`=blue, `declares`=amber, `uses_type`=purple, `contains`=emerald, `imports`=teal, `has_method`=pink, `returns`=orange, `module_file`=slate, `implements`=cyan, `possible_dispatch`=red.
- `possible` edges: dashed stroke. `rust_analyzer`/`mir` edges: glow effect + thicker.
- Arrow markers: 12×10px, white stroke, line endpoints trimmed to node radius so arrows sit outside circles.
- Edge kind filter: toggle pills with Chinese labels + colored dots, stored in localStorage, defaults to `calls` only.

### Layout (Web UI)
- Force-directed: repulsion constant 2800, ideal edge length 210 (calls) / 155 (declares), 150 iterations for small graphs.
- Node radius: 7–18px proportional to sqrt(degree).
- Center node pinned; others pulled by gravity (0.0012 for neighborhood mode).

### Web UI states
- Three state overlays: loading spinner, empty graph message, error message.
- Auto-detected from status: `starting` → loading, `failed` → error, no graph → empty.
- Error handling in `loadGraph` and `refreshStatus` with fallback rendering.

### Analyzer call resolution
- Function calls: prefer same-module resolution before cross-module.
- Method calls: only resolve to trait impl methods, not standalone functions.

### Semantic enrichment
- rust-analyzer auto-detected on PATH. Enabled by default; opt-out with `--no-semantic`.
- `--semantic-limit` controls max symbols scanned (default: 200).

## Key Conventions

- **Default graph output**: `<project>/.ferrimind/ferrimind.json`. Use `--output` to override.
- **Fixture project**: `tests/fixtures/sample/` — minimal Rust crate used by all integration tests.
- **Test pattern**: tests invoke the built binary via `std::process::Command`, index the fixture, assert on JSON responses.
- All CLI output is JSON.
- Edge provenance: `source` (ast/rust_analyzer/mir/inferred) × `certainty` (definite/confirmed/inferred/possible).

## Testing

- 6 unit tests in `src/query.rs` (ambiguous symbol resolution, file/module/symbol queries, path failure).
- 8 integration tests in `tests/cli.rs` (index, query, semantic, MIR, `--all`, profiles, self-index).
- All 14 tests pass. Run: `cargo test`.

## Known Limitations

- `hotspots` and `diff` require a git repository; fail gracefully otherwise.
- MIR mode lightly tested; requires nightly rustc with `RUSTC_BOOTSTRAP=1`.
- `--watch` hot-reload not thoroughly tested.
- Layout may become dense with 200+ visible nodes.
- Large projects (10k+ nodes) untested — indexing performance and graph rendering may degrade.
- proc macros and complex generics may produce incomplete call edges.

## Adding a New CLI Command

1. Identify group (`query`, `nav`, `analyze`, or top-level).
2. Add variant + arg struct to the subcommand enum in `cli.rs`.
3. Add match arm in `main.rs` under the group dispatch.
4. Implement in existing or new module.
5. Add test in `tests/cli.rs` using nested format: `"query" "search"`, `"nav" "guide"`, `"analyze" "deps"`.

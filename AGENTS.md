# crabmap

**Generated:** 2026-05-21 · **Commit:** 2ebf59e · **Branch:** main

Rust code knowledge graph for AI navigation. Single crate, edition 2024 (Rust ≥ 1.85), no workspace.

## Commands

```bash
cargo build --release      # optimized — enables build.rs git-info + progress bar
cargo run -- <subcommand>  # e.g. cargo run -- query search "load config"
cargo test                 # 6 unit + 8 integration tests
```

`build.rs` captures git commit + build date at compile time. Falls back to `"no-git"` outside a repo.

## Structure

```
crabmap/
├── src/           # 22 Rust modules, ~7500 lines — flat, all peers
│   ├── model.rs   # Central hub — all modules depend on this
│   ├── analyzer.rs  # Largest (1217L) — AST indexer
│   └── web.rs     # HTTP server + 15× include_str! for web/ assets
├── web/           # Embedded web UI — see web/AGENTS.md
├── tests/
│   ├── cli.rs     # Integration tests via std::process::Command
│   └── fixtures/sample/  # Minimal Rust crate for test indexing
└── build.rs       # Git info → cargo:rustc-env
```

## Where to Look

| Task | Location | Notes |
|---|---|---|
| Add CLI command | `cli.rs` → `main.rs` → new/existing module | See "Adding a New CLI Command" below |
| Change graph data model | `model.rs` | All modules depend on this — changes propagate |
| Fix symbol resolution | `query.rs` | `find_nodes()`: exact id > qualified_name > short name > suffix |
| Add indexing logic | `analyzer.rs` | syn AST walk, same-module priority for resolution |
| Change web UI | `web/` directory | See `web/AGENTS.md` — no build step, include_str! |
| Add LLM/RAG feature | `llm.rs`, `rag.rs`, `config.rs` | Config at `~/.config/crabmap/config.json` |
| Add static analysis | New module + `main.rs` dispatch | Under `analyze` command group |
| Fix error messages | Any module | Uses `anyhow::Result` + `.context()` throughout |
| Change terminal colors | `term.rs` | ANSI codes, auto-disabled when piped |

## Architecture

Single binary, all Rust modules in `src/`:

| Module | Lines | Purpose |
|---|---|---|
| `main.rs` | 411 | CLI entry, command dispatch, `index_project()` helper |
| `cli.rs` | 334 | clap definitions: 6 top-level commands, nested subcommand enums |
| `model.rs` | 297 | CodeGraph, Node, Edge, NodeKind, EdgeKind — core data model |
| `analyzer.rs` | 1217 | AST indexer: cargo metadata → syn walk → graph construction |
| `query.rs` | 836 | Adjacency index + traversal. `find_nodes()` 4-tier priority |
| `semantic.rs` | 638 | rust-analyzer LSP enrichment, auto-detected on PATH |
| `ai.rs` | 559 | AI navigation commands (guide, entries, clusters, quality, health, map) |
| `web.rs` | 510 | Embedded HTTP viewer, 15× `include_str!` for web/ assets |
| `rag.rs` | 386 | Retrieval: lexical search → embedding similarity → reranking |
| `llm.rs` | 369 | LLM client for `ask` command |
| `mir.rs` | 338 | rustc MIR text parsing for lowered calls |
| `cli.rs` | 334 | clap argument definitions |
| `report.rs` | 269 | GRAPH_REPORT.md and AGENT_GUIDE.md generation |
| `health.rs` | 264 | Architectural risk detection (cycles, god modules, dead code) |
| `config.rs` | 198 | Global LLM/RAG config (`~/.config/crabmap/config.json`) |
| `store.rs` | 169 | Gzip JSON load/save (`.crabmap/crabmap.json.gz`) |
| `gitintel.rs` | 153 | Git churn/ownership/co-change (requires git repo) |
| `deps.rs` | 128 | Module dependency direction analysis |
| `drift.rs` | 128 | Graph diff against git base |
| `repo_map.rs` | 116 | Token-budgeted repository map (~8k tokens) |
| `test_impact.rs` | 92 | Static test candidate discovery |
| `export.rs` | 80 | DOT/Mermaid/JSON export |
| `term.rs` | 36 | ANSI terminal colors with TTY detection |

## Module Dependencies

```
model.rs ← (all modules)
web.rs ← analyzer, cli, mir, model, query, semantic, store, term
main.rs ← cli (all subcommand structs)
term.rs, cli.rs — dependency-free (no crate:: imports)
```

## CLI Structure

```
crabmap
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

### Web UI
- Dark theme, microkernel architecture (CG namespace: state store + event bus).
- Edges colored by `kind` with Chinese-labeled filter pills. Force-directed layout.
- See `web/AGENTS.md` for full frontend architecture, API endpoints, and conventions.

### Analyzer call resolution
- Function calls: prefer same-module resolution before cross-module.
- Method calls: only resolve to trait impl methods, not standalone functions.

### Semantic enrichment
- rust-analyzer auto-detected on PATH. Enabled by default; opt-out with `--no-semantic`.
- `--semantic-limit` controls max symbols scanned (default: 200).

## Key Conventions

- **Default graph output**: `<project>/.crabmap/crabmap.json`. Use `--output` to override.
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

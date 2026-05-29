# src/analyzer/ — AST Indexer

**Generated:** 2026-05-29 · **Commit:** b9dae4c

Parses Rust source via `syn` AST walk, extracts nodes and edges, builds `CodeGraph`. 7 files (6 active + 1 stub), 1266 lines. Single entry point: `index_project()`.

## Structure

```
analyzer/
├── mod.rs          # Re-exports: index_project, IndexOptions
├── index.rs        # Main engine: index_project(), index_item(), index_impl() (673L)
├── builder.rs      # Builder struct: node creation, edge accumulation, resolution
├── types.rs        # IndexOptions, PendingEdge, ResolutionStrategy, NodeInput
├── visitors.rs     # syn::visit::Visit impl (FunctionCollector) — collects call edges
├── helpers.rs      # module_name(), file_metrics(), docs(), visibility(), location()
└── resolution.rs   # STUB — 2 lines, planned extraction from Builder
```

## Where to Look

| Task | File | Notes |
|---|---|---|
| Add node kind indexing | `index.rs::index_item()` | Giant match on `syn::Item` variants (322L god function) |
| Add edge type | `builder.rs::edge()` → `index.rs` | Builder accumulates, index.rs dispatches |
| Change call resolution | `builder.rs` resolution logic | Same-module priority, trait-only method resolution |
| Add AST visitor | `visitors.rs` | Implements `syn::visit::Visit` trait |
| Fix module naming | `helpers.rs::module_name()` | File path → qualified module name |
| Add index option | `types.rs::IndexOptions` | on_progress callback, file filters, semantic/mir flags |

## Indexing Pipeline

```
index_project(project, options)
  ├── cargo_metadata → find packages, targets, source roots
  ├── ignore::Walk → discover .rs files (respects .gitignore)
  ├── For each file:
  │   ├── syn::parse_file(source) → AST
  │   ├── index_file() → declares edges (file/module containment)
  │   ├── index_item() for each syn::Item:
  │   │   ├── Creates Node (struct/fn/enum/trait/impl/type/const/static/macro)
  │   │   ├── Adds Declares edge (owner → item)
  │   │   └── Collects PendingEdges (calls, type refs, impls, imports)
  │   └── FunctionCollector (syn Visit) → additional call edges from fn bodies
  └── Builder resolves PendingEdges → final CodeGraph
```

## Core Conventions

### Call Resolution (builder.rs resolution logic)

- **Function calls**: prefer same-module resolution before cross-module.
- **Method calls**: only resolve to trait impl methods, not standalone functions.
- **Resolution strategies** (`ResolutionStrategy` enum):
  - `Any` — match any node by name
  - `Callable` — only functions/methods
  - `MethodOnly` — only methods within trait impls
  - `MacroOnly` — only macro nodes
- Unresolved edges are dropped with a warning — no `#[allow]` suppression.

### Node ID Scheme

- IDs use `#` suffix for disambiguation: `crate::module::Struct#0`, `crate::module::Struct#1`.
- File IDs: `file:relative/path.rs`.
- Module IDs: `module:crate::path::to::module`.
- Owner ID for items: set to enclosing module or impl block.

### Progress Reporting

- `IndexOptions.on_progress: Option<Arc<dyn Fn(usize, usize) + Send + Sync>>` — callback for progress bars.
- File collection happens first (discovery phase), then indexed with progress bar (via `indicatif` in `main.rs`).
- Completion: `✓ indexed N nodes, M edges in F files` to stderr.

### Edge Provenance

All edges carry `source` (ast/rust_analyzer/mir/inferred) × `certainty` (definite/confirmed/inferred/possible):
- `ast` + `definite`: exact function call match in same module
- `ast` + `inferred`: trait method dispatch, macro expansions
- `rust_analyzer` + `confirmed`: LSP-verified call edge
- `mir` + `confirmed`: rustc MIR-lowered call edge

### Helpers (helpers.rs)

Pure functions, no state, no structs:
- `module_name(file, root)` → `"crate_name::path::to::item"`
- `visibility(vis)` → `"pub"`, `"pub(crate)"`, `"pub(super)"`, or empty
- `docs(attrs)` → first doc comment string
- `location(file, source, needle)` → line:column Location
- `file_metrics(source)` → line count, char count

## Known Limitations

- **Calls inside macros invisible**: `eprintln!("{}", fn())` — `syn` AST walk skips macro bodies. Semantic enrichment (rust-analyzer) fills some gaps.
- **proc macros / complex generics**: may produce incomplete or missing call edges.
- **Method resolution**: trait method calls to non-trait-impl methods won't resolve.
- **`resolution.rs` is a stub**: planned extraction of resolution logic from `Builder` — not yet implemented.

## Anti-Patterns (This Module)

- **`index_item()` god function** (322L): handles 10 `syn::Item` variants in a single match. Extract each variant to a dedicated function (`index_struct()`, `index_enum()`, etc.) following the existing `index_impl()` pattern.
- **`Builder` does too much**: node creation + edge accumulation + name resolution + edge dedup — all in one struct. Split resolution into the `resolution.rs` stub.
- **PendingEdge accumulation**: edges are accumulated in `builder.pending: Vec<PendingEdge>` and resolved at the end. This works but the Vec grows unbounded for large projects; consider streaming resolution.
- **No incremental indexing**: every `index_project()` call re-parses all files. No cache for unchanged files. Add mtime-based skip for files unchanged since last index.

## Key Types

| Type | Defined In | Purpose |
|---|---|---|
| `IndexOptions` | `types.rs` | File filters, progress callback, semantic/mir flags |
| `Builder` | `builder.rs` | Accumulates nodes + pending edges, resolves names |
| `PendingEdge` | `types.rs` | Unresolved edge: (from, to_name, kind, strategy) before resolution |
| `ResolutionStrategy` | `types.rs` | Any / Callable / MethodOnly / MacroOnly |
| `NodeInput` | `types.rs` | Raw node data before insertion into Builder |
| `FunctionCollector` | `visitors.rs` | syn Visit impl that collects call edges from fn bodies |

## Gotchas

- File IDs use `/` not `\` even on Windows — paths are normalized to Unix separators.
- `Builder::symbol()` auto-generates IDs with `#N` suffix for duplicate names.
- Module path construction in `helpers::module_name()` relies on `Cargo.toml` package name — mismatches cause wrong qualified names.
- `index_item()` is the most modified function in the codebase — test thoroughly after any change.
- Source files are read as UTF-8 strings; non-UTF-8 Rust files will fail at the `syn` parse step.

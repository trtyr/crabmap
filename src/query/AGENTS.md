# src/query/ — Query Engine

**Generated:** 2026-05-29 · **Commit:** b9dae4c

Reads and traverses the code graph built by `src/analyzer/`. Handles symbol resolution, call tracing, impact analysis, path finding, search, and export. 11 files, 903 lines.

## Structure

```
query/
├── mod.rs          # Re-exports 16 pub fns + SymbolFilter
├── commands.rs     # CLI command implementations (557L — god module)
├── find.rs         # find_nodes(), levenshtein(), suggest()
├── index.rs        # QueryIndex — O(1) adjacency index (HashMap-backed)
├── traversal.rs    # walk(), adjacent(), path() (petgraph astar)
├── ranking.rs      # hot_symbols(), ranked_nodes()
├── filter.rs       # SymbolFilter + apply()
├── risk.rs         # risk() — impact + test impact scoring
├── similar.rs      # similar() — embedding-based search
├── source.rs       # source() — extract source by id
├── refactor_order.rs  # Topological sort for safe refactor order (402L)
└── tests.rs        # 9 unit tests (#[cfg(test)])
```

## Where to Look

| Task | File | Notes |
|---|---|---|
| Add query command | `commands.rs` → `mod.rs` re-export | Follow existing fn signature pattern |
| Fix symbol resolution | `find.rs` | Priority: exact id > qualified_name > short name > suffix |
| Change graph traversal | `traversal.rs` | BFS for walk(), petgraph::astar for path() |
| Add ranking logic | `ranking.rs` | Degree-based + term matching |
| Add risk analysis | `risk.rs` | Calls `commands::impact()` internally |
| Add search feature | `similar.rs` | Embedding similarity via external API |
| Debug ambiguous symbols | `find.rs::suggest()` | Levenshtein distance ≤ 3 |

## Core Conventions

### Symbol Resolution (find.rs)

```
find_nodes() priority:
  1. Exact id match (with # suffix stripping)
  2. Exact qualified_name match (crate::module::name)
  3. Short name match (returns ALL matches)
  4. Suffix match (::name)
```

- **Ambiguous short names**: commands like `symbol`/`file`/`module` return `{"kind": "ambiguous", "matches": [...]}`. Traversal commands (`callees`/`callers`/`impact`/`path`) error out.
- **Not found**: Levenshtein ≤ 3 suggestions, format: `symbol 'X' not found\nDid you mean?\n  • Y`

### QueryIndex (index.rs)

Every query function constructs a temporary `QueryIndex::new(graph)`:
```rust
pub(crate) struct QueryIndex<'a> {
    nodes_by_id: HashMap<&'a str, &'a Node>,
    outbound: HashMap<&'a str, Vec<&'a Edge>>,
    inbound: HashMap<&'a str, Vec<&'a Edge>>,
    degree: HashMap<&'a str, usize>,
}
```
- **Not persistent** — rebuilt per query. Cheap (HashMap build: O(n)).
- **Edge filtering**: inbound edges use `is_call_like()` to exclude non-traversal edges (Declares, Contains).
- `AiIndex` in `src/ai/index.rs` is structurally identical — DRY if adding a third.

### Impact Analysis (commands.rs::impact())

- BFS both upstream (callers/dependents) and downstream (callees/dependencies).
- Inline risk scoring engine: file propagation × caller count × public API × method dispatch.
- Risk levels: low (0-3) / medium (4-9) / high (10-19) / critical (20+).
- `risk.rs` wraps `commands::impact()` with additional test impact analysis.

### Path Finding (traversal.rs)

- `path(from, to)` uses `petgraph::algo::astar` for shortest call path.
- `walk(root, direction, depth)` uses BFS with depth limit.
- `adjacent(id)` returns direct neighbors (1-hop callers + callees).

### Call Edge Detection

- `is_call_like()`: matches Calls, PossibleDispatch, UsesTrait, UsesType, HasMethod.
- `is_method_call()`: checks `call_style` field for "method" marker.
- Source/certainty aware: `rust_analyzer` + MIR edges carry extra weight in ranking.

## Anti-Patterns (This Module)

- **commands.rs is a god module** (557L, 12 fns spanning 4 domains): search, scope, traversal, and impact analysis share one file. Split into `search.rs` / `scope.rs` / `traversal.rs` / `impact.rs`.
- **`impact()` embeds a risk scoring engine** (206L inline). Extract to `risk.rs` or merge with existing `risk.rs`.
- **Duplicate `QueryIndex::new()` calls**: 10 of 12 command fns build their own index independently. Consider a shared context struct.
- **`unwrap()` in refactor_order.rs**: 5 calls in Tarjan SCC algorithm panic on lock failure. Use proper error propagation.
- **No async in search/similar**: `similar.rs` makes blocking HTTP calls via `reqwest::blocking`. Fine for CLI, problematic if called from web server.

## Key Types

| Type | Defined In | Purpose |
|---|---|---|
| `QueryIndex<'a>` | `index.rs` | O(1) adjacency lookup, rebuild per query |
| `SymbolFilter` | `filter.rs` | Filter by kind, visibility, dead code, caller |
| `FindMode` | `mod.rs` | Text vs semantic search mode |
| `TraceDirection` | `mod.rs` | Upstream (callers) vs downstream (callees) |
| `ScopeKind` | `mod.rs` | File vs module scope selector |

## Gotchas

- `find_nodes()` leaks an `Ambiguous` variant in its return type — callers must check or use `require_unique_node()`.
- `impact()` calls `walk()` twice (upstream + downstream) — no caching between invocations.
- Short name matches for names like "run" hit >5 nodes across modules. Use qualified names in scripts.
- `neighbors()` and `trace()` differ subtly: `neighbors` returns direct edges only, `trace` does depth-limited walk.
- Edge `source` field matters for ranking: `rust_analyzer` edges score higher than `ast` edges.

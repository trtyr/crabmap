# web/ — Embedded Web UI

**Generated:** 2026-05-21 · **Commit:** 2ebf59e

Dark-themed graph viewer embedded into the binary via `include_str!` in `src/web/assets.rs`. No build step, no npm, no bundler — pure vanilla JS + CSS loaded via `<script>` tags.

## Structure

```
web/
├── index.html            # HTML shell, loads all scripts/styles in order
├── styles/
│   ├── base.css          # CSS variables (dark palette), reset, typography
│   ├── layout.css        # Grid: sidebar (260px) + canvas + drawer (380px)
│   ├── components.css    # .btn, .pill, .metric, .edge-filter-pill, .kv
│   └── graph.css         # SVG: nodes, edges, overlays, spinner, legend
└── src/
    ├── core.js           # CG namespace: state store + event bus (load FIRST)
    ├── utils.js           # nodeColor(), edgeColor(), edgeLegend(), helpers
    ├── api.js             # HTTP client for /api/* endpoints
    ├── graph-layout.js    # Seeded positions + force-directed relaxation
    ├── graph-render.js    # SVG rendering engine (largest JS file, 307L)
    ├── graph-interact.js  # Drag, pan, zoom handlers
    ├── sidebar.js         # Left panel: search results, edge filters, metrics
    ├── details.js         # Right drawer: node/edge properties
    ├── toolbar.js         # Top controls: search, depth, reindex, status
    └── main.js            # Bootstrap — registers all modules, starts app
```

## Where to Look

| Task | File | Notes |
|---|---|---|
| Add API endpoint call | `api.js` + `web/server.rs` | Frontend client in api.js, backend handler in web/server.rs |
| Change graph layout | `graph-layout.js` | Force-directed params: repulsion 2800, edge length 210/155 |
| Change node/edge rendering | `graph-render.js` | SVG creation, color maps per kind |
| Add UI panel/widget | New JS + CSS | Register via `CG.register(init)`, add styles to components.css |
| Change layout/grid | `layout.css` | Sidebar 260px, drawer 380px, flex layout |
| Change colors/theme | `base.css` | CSS custom properties at :root |
| Add state field | `core.js` | Add to initial `state` object, update via `CG.setState()` |
| Add event type | `core.js` | Emit via `CG.emit()`, subscribe via `CG.on()` |

## Architecture: Microkernel (CG Namespace)

Global `window.CG` IIFE provides two primitives:

1. **State Store** — `CG.setState(patch)` merges into central state, emits `state:change`
2. **Event Bus** — `CG.on(event, fn)`, `CG.emit(event, data)`, `CG.off(event, fn)`

Modules attach as `CG.moduleName = { ... }`. **Load order matters** — `core.js` first, then utils/api, then features, finally `main.js`.

### State Fields

```
status, graph, nodesById, nodePositions, rootId, selected, searchItems, view: {x,y,scale}, drag
```

### Custom Events

`node:select`, `edge:select`, `view:change`, `filters:change`, `search:results`, `graph:loaded`

## API Endpoints

| Method | Endpoint | Used By |
|---|---|---|
| `GET` | `/api/status` | toolbar (polls every 1.5s) |
| `GET` | `/api/graph` | main.js on `ready` |
| `GET` | `/api/search?q=&limit=` | sidebar search |
| `GET` | `/api/symbol?id=` | details drawer |
| `GET` | `/api/callees?id=&depth=` | neighborhood expansion |
| `GET` | `/api/callers?id=&depth=` | neighborhood expansion |
| `GET` | `/api/impact?id=&depth=` | impact visualization |
| `POST` | `/api/reindex` | toolbar reindex button |

## Conventions

- **No ES modules** — all files use global `CG` namespace via IIFE
- **Chinese UI** — hardcoded zh-CN labels (就绪, 加载中, 调用, 声明, etc.)
- **localStorage** — edge filter state persisted as `cg-edge-kinds`
- **Auto-select** — on load, automatically selects `crabmap::run` node
- **Dark theme only** — CSS variables define the entire palette in `base.css`

## Edge Color Map

| Kind | Color | CSS Class |
|---|---|---|
| calls | blue | `.edge-calls` |
| declares | amber | `.edge-declares` |
| uses_type | purple | `.edge-uses_type` |
| contains | emerald | `.edge-contains` |
| imports | teal | `.edge-imports` |
| has_method | pink | `.edge-has_method` |
| returns | orange | `.edge-returns` |
| module_file | slate | `.edge-module_file` |
| implements | cyan | `.edge-implements` |
| possible_dispatch | red (dashed) | `.edge-possible_dispatch` |

`rust_analyzer`/`mir` source edges: glow effect + thicker stroke.

## Gotchas

- `include_str!` means **any web/ change requires `cargo build`** to take effect (assets embedded in `src/web/assets.rs`)
- Script load order in `index.html` is critical — `core.js` must be first
- Toolbar polls `/api/status` every 1.5s — check network tab if UI seems stale
- Force-directed layout runs 90–150 iterations client-side; large graphs (200+ nodes) may lag

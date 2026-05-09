# 🦀 Ferrimind

<p align="center">
  <strong>Rust Code Knowledge Graph for AI Navigation</strong>
  <br>
  <a href="README_zh.md">🇨🇳 中文文档</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-1.85%2B-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License">
  <img src="https://img.shields.io/badge/status-active-brightgreen.svg" alt="Status">
</p>

---

**Ferrimind** builds a durable, queryable knowledge graph of any Rust project. Give it to an AI agent — it understands the entire codebase without reading files one by one.

Think of it as a **satellite map** of your Rust codebase, while LSP (rust-analyzer) gives you a **microscope** for individual symbols.

---

## ✨ Why Ferrimind?

| LSP / rust-analyzer | Ferrimind |
|:---|:---|
| One symbol at a time | **Whole-project graph** in one shot |
| Needs an open IDE | **Offline, portable JSON** file |
| No "project overview" | **`nav map`** — 8000-token architecture summary |
| Find references (flat list) | **`query impact`** — full dependency propagation chains |
| No structural health checks | **`analyze health`** — cycles, god modules, dead code |
| Human-first (hover, click) | **AI-first** — designed for LLM context windows |

---

## 🚀 Quick Start

```bash
# Install
cargo install ferrimind

# Index a project
ferrimind index /path/to/rust/project
# ✓ indexed 9089 nodes, 14355 edges in 168 files

# Get an AI-ready architecture map
ferrimind nav map

# Search for anything
ferrimind query search "handle_connection"

# Explore interactively in the browser
ferrimind serve
```

---

## 📦 Commands

### `ferrimind index` — Build the graph

```bash
ferrimind index .                           # Index current project
ferrimind index --all .                     # Discover & index all Cargo projects
ferrimind index --no-tests                  # Skip test files
ferrimind index --output custom.json.gz     # Custom output path (gzipped)
```

### `ferrimind query` — Ask questions

```bash
ferrimind query stats                       # Node/edge counts
ferrimind query search "config"             # Fuzzy text search
ferrimind query symbol main                 # Inspect a symbol
ferrimind query callees main --depth 3      # What does main call?
ferrimind query callers load_config         # Who calls this?
ferrimind query impact Runtime --depth 2    # Full dependency impact
ferrimind query path main load_config       # Shortest call path
```

### `ferrimind nav` — AI-oriented navigation

```bash
ferrimind nav map           # Token-budgeted project overview (for LLMs)
ferrimind nav guide         # Entry points + call chains
ferrimind nav clusters      # Feature clusters by file
ferrimind nav quality       # Graph confidence score
ferrimind nav health        # Cycles, god modules, dead code
```

### `ferrimind analyze` — Static analysis

```bash
ferrimind analyze deps      # Module dependency matrix
ferrimind analyze fanout    # File-level fan-in / fan-out
ferrimind analyze tests     # Test impact candidates
ferrimind analyze hotspots  # Git churn hotspots
ferrimind analyze diff      # Graph diff vs git base
```

### `ferrimind serve` — Web UI

```bash
ferrimind serve                         # Index + serve
ferrimind serve --graph graph.json.gz   # Serve a pre-built graph
ferrimind serve --watch                 # Auto-reindex on file changes
```

### `ferrimind config` — API Keys (for LLM features)

```bash
ferrimind config --api-key sk-... --model gpt-4
```

---

## 🌐 Web UI

Launch `ferrimind serve` and open `http://127.0.0.1:7878`:

- **Graph visualization** — force-directed layout, color-coded by node/edge kind
- **Interactive exploration** — click nodes to expand, drag to rearrange
- **Edge filtering** — toggle relationship types (calls, declares, uses_type, …)
- **Detail drawer** — inspect symbols, files, edges
- **Chinese UI** — full zh-CN interface

---

## 🧪 Tested On

| Project | Nodes | Edges | Warnings | Quality |
|:---|--:|--:|:--:|:--:|
| ferrimind (self) | 899 | 1,676 | 0 | 99 |
| ripgrep | 9,089 | 14,355 | 0 | 96 |
| tokio | 14,176 | 28,831 | 0 | 98 |

All three projects indexed with **zero warnings**.

---

## 🔧 How It Works

1. **`cargo metadata`** → discovers packages, targets, source files
2. **`syn` AST walk** → extracts structs, enums, functions, methods, impls, macros…
3. **Call resolution** → same-module priority, method-to-trait-impl matching
4. **rust-analyzer enrichment** (optional) → LSP call hierarchy for confirmed edges
5. **MIR lowering** (optional) → rustc MIR for dispatch sites
6. **Graph persistence** → gzip-compressed JSON (14× smaller than raw)

---

## 📄 Graph Format

The output is a single JSON file (gzipped by default):

```json
{
  "schema_version": 2,
  "project": { "root": ".", "packages": […] },
  "nodes": [
    { "id": "function:ferrimind::run", "kind": "function", "name": "run", … }
  ],
  "edges": [
    { "from": "function:ferrimind::main", "to": "function:ferrimind::run",
      "kind": "calls", "source": "ast", "certainty": "definite" }
  ]
}
```

| Edge Kind | Meaning |
|:---|:---|
| `calls` | Function/method call |
| `declares` | Module declares a symbol |
| `uses_type` | Type reference |
| `contains` | File contains a module |
| `imports` | `use` statement |
| `has_method` | Impl block owns a method |
| `implements` | Trait implementation |
| `returns` | Return type |
| `module_file` | File ↔ module mapping |

---

## 🛠 Building from Source

```bash
git clone https://github.com/yourname/ferrimind.git
cd ferrimind
cargo build --release
./target/release/ferrimind --version
# ferrimind 0.1.0 (abc1234 2026-05-09)
```

Requires Rust ≥ 1.85 (edition 2024).

---

## 📁 Project Layout

```
src/
├── main.rs          # CLI entry & command dispatch
├── cli.rs           # clap argument definitions
├── analyzer.rs      # AST indexer (syn walk)
├── query.rs         # Graph traversal & search
├── model.rs         # Core data model
├── store.rs         # Gzip JSON load/save
├── web.rs           # Embedded HTTP server
├── semantic.rs      # rust-analyzer enrichment
├── mir.rs           # MIR lowering
├── ai.rs            # AI nav commands
├── config.rs        # Global config (~/.config/ferrimind/)
├── term.rs          # ANSI terminal colors
├── health.rs        # Architecture risk detection
└── …

web/
├── index.html
├── styles/          # CSS (dark theme)
└── src/             # JS (microkernel architecture)
```

---

## 📝 License

MIT

---

<p align="center">
  <sub>Built for AI. Made with 🦀 in Rust.</sub>
</p>

# 🦀 Crabmap

<p align="center">
  <strong>Rust code satellite map — index, query, and navigate your codebase</strong>
  <br>
  <a href="README_zh.md">🇨🇳 中文文档</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-1.85%2B-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License">
  <img src="https://img.shields.io/crates/v/crabmap.svg" alt="crates.io">
  <img src="https://img.shields.io/badge/status-active-brightgreen.svg" alt="Status">
</p>

---

**Crabmap** builds a durable, queryable knowledge graph of any Rust project. Give it to an AI agent — it understands the entire codebase without reading files one by one.

Think of it as a **satellite map** of your Rust codebase, while LSP (rust-analyzer) gives you a **microscope** for individual symbols.

---

## ✨ Why Crabmap?

| LSP / rust-analyzer | Crabmap |
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
cargo install crabmap

# Index a project
crabmap index /path/to/rust/project
# ✓ indexed 9089 nodes, 14355 edges in 168 files

# Get an AI-ready architecture map
crabmap nav map

# Search for anything
crabmap query search "handle_connection"

# Explore interactively in the browser
crabmap serve
```

---

## 📦 Commands

### `crabmap index` — Build the graph

```bash
crabmap index .                           # Index current project
crabmap index --all .                     # Discover & index all Cargo projects
crabmap index --no-tests                  # Skip test files
crabmap index --output custom.json.gz     # Custom output path (gzipped)
```

### `crabmap query` — Ask questions

```bash
crabmap query stats                       # Node/edge counts
crabmap query search "config"             # Fuzzy text search
crabmap query symbol main                 # Inspect a symbol
crabmap query callees main --depth 3      # What does main call?
crabmap query callers load_config         # Who calls this?
crabmap query impact Runtime --depth 2    # Full dependency impact
crabmap query path main load_config       # Shortest call path
```

### `crabmap nav` — AI-oriented navigation

```bash
crabmap nav map           # Token-budgeted project overview (for LLMs)
crabmap nav guide         # Entry points + call chains
crabmap nav clusters      # Feature clusters by file
crabmap nav quality       # Graph confidence score
crabmap nav health        # Cycles, god modules, dead code
```

### `crabmap analyze` — Static analysis

```bash
crabmap analyze deps      # Module dependency matrix
crabmap analyze fanout    # File-level fan-in / fan-out
crabmap analyze tests     # Test impact candidates
crabmap analyze hotspots  # Git churn hotspots
crabmap analyze diff      # Graph diff vs git base
```

### `crabmap serve` — Web UI

```bash
crabmap serve                         # Index + serve
crabmap serve --graph graph.json.gz   # Serve a pre-built graph
crabmap serve --watch                 # Auto-reindex on file changes
```

### `crabmap config` — API Keys (for LLM features)

```bash
crabmap config --api-key sk-... --model gpt-4
```

---

## 🌐 Web UI

Launch `crabmap serve` and open `http://127.0.0.1:7878`:

- **Graph visualization** — force-directed layout, color-coded by node/edge kind
- **Interactive exploration** — click nodes to expand, drag to rearrange
- **Edge filtering** — toggle relationship types (calls, declares, uses_type, …)
- **Detail drawer** — inspect symbols, files, edges
- **Chinese UI** — full zh-CN interface

---

## 🧪 Tested On

| Project | Nodes | Edges | Warnings | Quality |
|:---|--:|--:|:--:|:--:|
| crabmap (self) | 899 | 1,676 | 0 | 99 |
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
    { "id": "function:crabmap::run", "kind": "function", "name": "run", … }
  ],
  "edges": [
    { "from": "function:crabmap::main", "to": "function:crabmap::run",
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
git clone https://github.com/trtyr/crabmap.git
cd crabmap
cargo build --release
./target/release/crabmap --version
# crabmap 0.1.1 (abc1234 2026-05-21)
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
├── config.rs        # Global config (~/.config/crabmap/)
├── term.rs          # ANSI terminal colors
├── health.rs        # Architecture risk detection
└── …

web/
├── index.html
├── styles/          # CSS (dark theme)
└── src/             # JS (microkernel architecture)

skills/
└── crabmap.md     # AI agent usage guide (skill file)
```

---

## 📝 License

MIT

---

<p align="center">
  <sub>Built for AI. Made with 🦀 in Rust.</sub>
</p>

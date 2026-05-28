---
name: crabmap
description: |
  Rust codebase satellite map. Indexes Rust project to build call graph, then
  search symbols, trace call chains, analyze dependency impact, check
  architecture health, and export LLM-optimized nav map (~8k tokens).

  LOAD WHEN: You enter ANY Rust project (has Cargo.toml or .rs files) and
  need to understand structure, find a function/struct/trait, trace callers
  or callees, analyze dependency impact of a change, or check for circular
  dependencies and dead code. Load BEFORE doing manual glob/grep/read
  exploration — crabmap gives you the architectural overview much faster.

  DO NOT: Manually glob/grep/read your way through a Rust codebase to
  understand its structure when this skill exists. One `nav map` call
  replaces dozens of manual file reads.

  TRIGGERS: Rust, Cargo.toml, .rs, crate, this project, project structure,
  architecture, call graph, callers, callees, dependency chain, fan-in,
  fan-out, circular dependency, dead code, nav map, crabmap
---

# Crabmap — Rust Code Knowledge Graph Skill

## What It Is

Crabmap indexes a Rust project into a structured call graph (JSON), enabling search, query, navigation, and analysis without reading source files one by one.

**Mental model**: You are the satellite map. LSP is the microscope. Look at the map first, then walk. Don't fumble in the dark.

---

## Priority Rule (Highest Priority)

**When entering a Rust project, crabmap is the first tool, not a fallback.**

When you need to:
- Understand project structure / find entry points / grasp architecture
- Locate a function, type, or module
- Trace call relationships (who calls whom, impact scope)
- Check dependencies, circular references, code health
- Figure out "how is this project organized"

**You MUST use crabmap first, not manual grep, glob, or read.**

```
❌ Wrong:
   grep keywords → read files → grep again → read more → getting lost

✅ Right:
   crabmap index .                   ← build the graph once
   crabmap nav map                   ← see the big picture
   crabmap query search "X"          ← locate your target
   crabmap query callers/callees X   ← trace relationships
   → the graph tells you which file to read, then use read for the code
```

**Only skip crabmap when:**
- The project has only 1-2 source files (no map needed)
- You've already located the specific file via crabmap and need to read implementation details
- It's not a Rust project (crabmap only supports Rust)

---

## Installation

```bash
cargo install crabmap
crabmap --version  # verify installation
```

---

## Core Workflow

```
1. crabmap index /path/to/project     → build the graph (.json.gz)
2. crabmap query stats                → quickly check graph size
3. crabmap nav map                    → project architecture summary (for AI)
4. crabmap query search "xxx"         → search for interesting things
5. crabmap query callees/callers/impact/path → deep trace
6. crabmap analyze health/deps/fanout → structural health check
```

---

## Command Reference

### `crabmap index` — Build the Graph

```bash
crabmap index .                           # index current project
crabmap index --all .                     # all crates in workspace
crabmap index --no-tests                  # skip test files
crabmap index --output custom.json.gz     # custom output path
crabmap index --no-semantic               # skip semantic analysis (faster, less info)
```

After indexing, the graph is saved at `.crabmap/crabmap.json.gz`. All subsequent commands auto-discover and load it.

**Multiple projects**: `cd` to different directories to auto-load their graphs. Use `--graph <FILE>` to specify explicitly.

---

### `crabmap query` — Query the Graph

All query commands support `--graph <FILE>`, `--depth`, `--limit`.

#### Stats & Overview

```bash
crabmap query stats                      # node/edge/file counts
crabmap query summary                    # project summary
crabmap query symbols                    # list all symbols
crabmap query symbols --kind function    # functions only
```

#### Symbols & Files

```bash
crabmap query symbol <NAME>              # detailed symbol info
crabmap query file <PATH>                # all symbols in a file
crabmap query module <NAME>              # symbols in a module
```

#### Call Tracing (Most Used)

```bash
crabmap query callees <NAME> --depth 3   # who does NAME call? (downstream)
crabmap query callers <NAME> --depth 3   # who calls NAME? (upstream)
crabmap query impact <NAME> --depth 2    # full dependency impact of NAME
crabmap query path <FROM> <TO>           # shortest call path between two symbols
```

#### Search & Export

```bash
crabmap query search "handle_conn"       # fuzzy text search
crabmap query export                     # export JSON
crabmap query export --format dot        # export Graphviz DOT
crabmap query export --format mermaid    # export Mermaid diagram
```

---

### `crabmap nav` — AI Navigation (Core)

This group is designed for AI agents, with token-budget-optimized output.

```bash
crabmap nav map           # project architecture summary (~8000 tokens, ready for context)
crabmap nav guide         # entry points + call chains
crabmap nav entries       # list all entry points (public API)
crabmap nav clusters      # feature clusters by file
crabmap nav quality       # graph quality score (confidence)
crabmap nav health        # architecture health: cycles, god modules, dead code
```

#### AI-Enhanced Queries (Optional, requires API key)

```bash
crabmap nav ask "Where is the authentication logic in this project?"
crabmap nav retrieve "authentication middleware"
```

Configure first:
```bash
crabmap config --api-key "sk-..." --model "gpt-4o"
crabmap config --embedding-key "..." --embedding-model "text-embedding-3-small"
```

---

### `crabmap analyze` — Static Analysis

```bash
crabmap analyze deps                     # module dependency matrix
crabmap analyze fanout                   # file-level fan-in / fan-out
crabmap analyze tests                    # test impact analysis
crabmap analyze hotspots                 # git churn hotspots
crabmap analyze diff                     # graph diff vs git base
```

---

### `crabmap serve` — Web Viewer

```bash
crabmap serve                            # start web server (127.0.0.1:7878)
crabmap serve --port 3000                # custom port
crabmap serve --watch                    # watch for file changes, auto-rebuild
```

---

## Output Format Reference

All CLI output is JSON. Every command returns a top-level `kind` field (except `query export --format json`).

### Common Output Structures

| Command | kind | Key Fields |
|---------|------|------------|
| `query stats` | `"stats"` | `stats.nodes`, `stats.edges`, `stats.by_kind`, `stats.by_edge` |
| `query summary` | `"summary"` | `hot_symbols[]`, `project`, `stats`, `top_files[]` |
| `query symbols` | `"symbols"` | `items[].id/name/kind/file` |
| `query symbol NAME` | `"symbol"` | `node{}`, `incoming[]`, `outgoing[]` |
| `query file PATH` | `"file"` | `declares[]` |
| `query module NAME` | `"module"` | `declares[]` |
| `query callees/callers` | `"callees"`/`"callers"` | `items[].edge`, `items[].node` |
| `query impact` | `"impact"` | `root`, `dependencies[]`, `dependents[]`, `callers[]` |
| `query search` | `"search"` | `items[]` |
| `query path` | `"path"` | `found: bool`, `nodes[]`, `from{}`, `to{}` |
| `query export --format mermaid` | `"mermaid"` | `content: "graph LR..."` |
| `query export --format dot` | `"dot"` | `content: "digraph codegraph {..."` |
| `query export --format json` | no `kind` | `nodes`, `edges`, `project`, `schema_version` |
| `nav map` | `"map"` | `content`, `budget: 8000` |
| `nav entries` | `"entries"` | `items[]` |
| `nav clusters` | `"clusters"` | `items[]` |
| `nav quality` | `"quality"` | `score: N` |
| `nav health` | `"health"` | `score: N` |
| `nav guide` | `"guide"` | `read_order[]` |
| `analyze deps` | `"deps"` | `items[].from/to/weight` |
| `analyze fanout` | `"fanout"` | `items[].file/fanin/fanout/total` |
| `analyze tests` | `"tests"` | `candidate_tests[]`, `targets[]`, `note` |
| `analyze hotspots` | `"git"` | `hotspots[]`, `cochange[]`, `repo` |
| `analyze diff` | `"diff"` | `added_edges[]`, `removed_edges[]` (string arrays), `changed_files[]` |
| `config` | `"config"` | `config{}`, `path` |

### Error Handling

**Symbol not found**: exit code 1, stderr contains:
```
error: symbol 'X' not found
Did you mean?
  • similar_name_1
  • similar_name_2
```

**Ambiguous symbol**: exit code 0, stdout returns JSON:
```json
{"kind": "ambiguous", "matches": [{"id": "...", "qualified_name": "crate::module::name", ...}]}
```

**Path not found**: `query path` returns `{"kind": "path", "found": false}`, exit code 0.

---

## Scenario Quick Reference

| You want to | Use |
|-------------|-----|
| "What is the overall project structure?" | `crabmap nav map` |
| "Where is this function called from?" | `crabmap query callers` |
| "What breaks if I change function A?" | `crabmap query impact A --depth 3` |
| "How do I get from entry to function X?" | `crabmap query path main handle_request` |
| "Are there circular dependencies?" | `crabmap nav health` |
| "Which modules have the heaviest deps?" | `crabmap analyze fanout` |
| "Quick keyword search" | `crabmap query search "keyword"` |
| "Show someone the structure" | `crabmap query export --format mermaid` |
| "Read actual code" | Use `read` tool directly |
| "Symbol jump/completion" | Use `lsp` tool (crabmap doesn't replace LSP) |

---

## Combined Usage

Crabmap and LSP complement each other, not replace:

1. First use `crabmap nav map` to understand global architecture
2. Use `crabmap query search` to locate symbols of interest
3. Use `crabmap query callers/callees` to trace call relationships
4. When you need implementation details, use LSP `goToDefinition` or `read`

---

## Troubleshooting

```bash
# graph file not found
crabmap index .          # index first

# indexing too slow
crabmap index --no-semantic --no-tests .

# serve port conflict
crabmap serve --port 3000

# AI features unavailable
crabmap config --api-key "sk-..."
```

---

## How It Works

Crabmap parses Rust source code AST via `syn`, extracting:
- **Nodes**: functions, structs, enums, traits, impls, modules, files, etc.
- **Edges**: function calls, type references, module imports, trait implementations, etc.

The graph is stored as a gzip JSON file, using `petgraph` for graph algorithms (path search, topological sort, etc.).

`nav map` output is token-budget-optimized (default ~8000 tokens), designed for LLM context windows.

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

# Crabmap — Rust 代码知识图谱 Skill

## 这是什么

Crabmap 把 Rust 项目整体索引成一个结构化调用图（JSON），然后你可以在上面搜索、查询、导航、分析——不需要逐个文件读源码。

**心智模型**：你是项目的卫星地图，LSP 是显微镜。先看地图再走路，不要摸着石头过河。

---

## 优先级规则（最高优先级）

**遇到 Rust 项目时，crabmap 是第一入口，不是备选方案。**

当你需要对一个 Rust 项目做以下操作时——
- 理解项目结构 / 找入口点 / 了解架构
- 搜索某个函数、类型、模块在哪里
- 追踪调用关系（谁调用了谁、影响范围）
- 检查依赖、循环引用、代码健康度
- 搞清楚「这个项目是怎么组织的」

**你必须先用 crabmap，而不是手动 grep、glob、read 文件。**

```
❌ 错误：
   grep 搜关键字 → read 打开文件 → 再 grep → 再 read → 越看越乱

✅ 正确：
   crabmap index .           ← 一次性构建图谱
   crabmap nav map           ← 看清全局结构
   crabmap query search "X"  ← 定位目标
   crabmap query callers/callees X  ← 追踪关系
   → 图谱告诉你该看哪个文件，再去 read 看具体代码
```

**只有以下情况才跳过 crabmap：**
- 项目只有 1-2 个源文件（不需要地图）
- 你已经通过 crabmap 定位到了具体文件，现在要看实现细节
- 非 Rust 项目（crabmap 只支持 Rust）

---

## 安装

```bash
cargo install crabmap
crabmap --version  # 确认安装成功
```

---

## 核心工作流

```
1. crabmap index /path/to/project     → 生成知识图谱 (.json.gz)
2. crabmap query stats                 → 快速了解图规模
3. crabmap nav map                     → 获取项目架构摘要 (给 AI 看)
4. crabmap query search "xxx"          → 搜索感兴趣的东西
5. crabmap query callees/callers/impact/path → 深入追踪
6. crabmap analyze health/deps/fanout  → 结构健康检查
```

---

## 命令详解

### `crabmap index` — 构建图谱

```bash
crabmap index .                           # 索引当前项目
crabmap index --all .                     # workspace 全部 crate
crabmap index --no-tests                  # 跳过测试文件
crabmap index --output custom.json.gz     # 自定义输出路径
crabmap index --no-semantic               # 跳过语义分析（更快但信息少）
```

索引完成后，图谱保存在 `.crabmap/crabmap.json.gz`。后续所有命令自动发现并加载。

**多项目**：`cd` 到不同目录会自动加载那个项目的图。也可用 `--graph <FILE>` 显式指定。

---

### `crabmap query` — 查询图谱

所有查询命令支持 `--graph <FILE>`、`--depth`、`--limit`。

#### 统计与概览

```bash
crabmap query stats                      # 节点数、边数、文件数
crabmap query summary                    # 项目摘要
crabmap query symbols                    # 列出所有符号
crabmap query symbols --kind function    # 只看函数
```

#### 符号与文件

```bash
crabmap query symbol <NAME>              # 查看某个符号详情
crabmap query file <PATH>                # 查看某文件的所有符号
crabmap query module <NAME>              # 查看某模块
```

#### 调用关系（最常用）

```bash
crabmap query callees <NAME> --depth 3   # NAME 调用了谁？（下游）
crabmap query callers <NAME> --depth 3   # 谁调用了 NAME？（上游）
crabmap query impact <NAME> --depth 2    # NAME 的完整依赖影响范围
crabmap query path <FROM> <TO>           # 从 FROM 到 TO 的最短调用路径
```

#### 搜索与导出

```bash
crabmap query search "handle_conn"       # 模糊文本搜索
crabmap query export                     # 导出 JSON
crabmap query export --format dot        # 导出 Graphviz DOT
crabmap query export --format mermaid    # 导出 Mermaid 图
```

---

### `crabmap nav` — AI 导航（核心）

这组命令为 AI agent 设计，输出经过 token 预算优化。

```bash
crabmap nav map           # 项目架构摘要 (~8000 tokens，可直接塞 context)
crabmap nav guide         # 入口点 + 调用链
crabmap nav entries       # 列出所有入口点 (public API)
crabmap nav clusters      # 按文件聚类 feature
crabmap nav quality       # 图谱质量评分 (置信度)
crabmap nav health        # 架构健康：循环依赖、god modules、死代码
```

#### AI 增强查询（可选，需配置 API key）

```bash
crabmap nav ask "这个项目的认证逻辑在哪里？"
crabmap nav retrieve "authentication middleware"
```

需要先配置：
```bash
crabmap config --api-key "sk-..." --model "gpt-4o"
crabmap config --embedding-key "..." --embedding-model "text-embedding-3-small"
```

---

### `crabmap analyze` — 静态分析

```bash
crabmap analyze deps                     # 模块依赖矩阵
crabmap analyze fanout                   # 文件级 fan-in / fan-out
crabmap analyze tests                    # 测试影响分析
crabmap analyze hotspots                 # Git churn 热点
crabmap analyze diff                     # 图谱 diff (对比 git base)
```

---

### `crabmap serve` — Web 交互

```bash
crabmap serve                            # 启动 Web 服务 (127.0.0.1:7878)
crabmap serve --port 3000                # 自定义端口
crabmap serve --watch                    # 监听文件变化自动重建
```

---

## 输出格式参考

所有 CLI 输出都是 JSON。每个命令返回的顶层 JSON 都有 `kind` 字段标识类型（`query export --format json` 除外）。

### 常用输出结构

| 命令 | kind | 关键字段 |
|------|------|----------|
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
| `query export --format json` | 无 `kind` | `nodes`, `edges`, `project`, `schema_version` |
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
| `analyze diff` | `"diff"` | `added_edges[]`, `removed_edges[]`（字符串数组）, `changed_files[]` |
| `config` | `"config"` | `config{}`, `path` |

### 错误处理

**符号未找到**：exit code 1，stderr 包含：
```
error: symbol 'X' not found
Did you mean?
  • similar_name_1
  • similar_name_2
```

**符号歧义**：exit code 0，stdout 返回 JSON：
```json
{"kind": "ambiguous", "matches": [{"id": "...", "qualified_name": "crate::module::name", ...}]}
```

**路径未找到**：`query path` 返回 `{"kind": "path", "found": false}`，exit code 0。

---

## 场景决策表

| 你想做什么 | 用什么命令 |
|------------|------------|
| "这个项目整体结构是什么样的？" | `crabmap nav map` |
| "这个函数被哪些地方调用了？" | `crabmap query callers` |
| "改了函数 A，会影响哪些东西？" | `crabmap query impact A --depth 3` |
| "从入口到某个函数怎么走？" | `crabmap query path main handle_request` |
| "有没有循环依赖？" | `crabmap nav health` |
| "哪些模块依赖最重？" | `crabmap analyze fanout` |
| "快速定位某个关键词" | `crabmap query search "keyword"` |
| "给别人看项目结构图" | `crabmap query export --format mermaid` |
| "看具体代码实现" | 用 `read` 工具直接看源码 |
| "符号跳转/补全" | 用 `lsp` 工具（crabmap 不替代 LSP） |

---

## 组合使用

crabmap 和 LSP 互补，不是替代：

1. 先用 `crabmap nav map` 了解全局架构
2. 用 `crabmap query search` 定位感兴趣的符号
3. 用 `crabmap query callers/callees` 追踪调用关系
4. 需要看具体实现时，再用 LSP `goToDefinition` 或 `read` 看源码

---

## 故障排查

```bash
# 图文件找不到
crabmap index .          # 先索引

# 索引太慢
crabmap index --no-semantic --no-tests .

# serve 端口被占用
crabmap serve --port 3000

# AI 功能不可用
crabmap config --api-key "sk-..."
```

---

## 原理简述

Crabmap 通过 `syn` 解析 Rust 源码 AST，提取：
- **节点 (Nodes)**：函数、结构体、枚举、trait、impl、模块、文件等
- **边 (Edges)**：函数调用、类型引用、模块导入、trait 实现等

图谱存储在 gzip JSON 文件中，用 `petgraph` 做图算法（路径搜索、拓扑排序等）。

`nav map` 的输出经过 token 预算优化（默认 ~8000 tokens），专门适配 LLM 的 context window。

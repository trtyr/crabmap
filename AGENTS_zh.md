# crabmap

Rust 代码知识图谱，为 AI 导航设计。单 crate，非 workspace。

## 构建与运行

```bash
cargo build                # debug
cargo build --release      # 优化构建（推荐）— 启用 build.rs git-info + 进度条
cargo run -- <子命令>       # 例：cargo run -- query search "load config"
cargo test                 # 6 个单元测试 + 8 个集成测试
```

Edition 2024 — 需要较新的 stable Rust 工具链（≥ 1.85）。

`build.rs` 在编译时捕获 git commit 和构建日期，供 `--version` 使用。非 git 仓库回退为 `no-git`。

## 架构

单二进制文件，所有 Rust 模块位于 `src/`：

| 模块 | 用途 |
|---|---|
| `main.rs` | CLI 入口，命令分发，`index_project()` 辅助函数 |
| `cli.rs` | clap 定义：6 个顶层命令（`index`、`serve`、`query`、`nav`、`analyze`、`config`），各含嵌套子命令枚举 |
| `model.rs` | CodeGraph、Node、Edge、NodeKind、EdgeKind — 核心数据模型 |
| `analyzer.rs` | AST 索引器：cargo metadata → syn 遍历 → 图构建。函数/方法调用优先同模块匹配 |
| `store.rs` | 图谱 JSON 加载/保存，默认路径（`.crabmap/crabmap.json.gz`） |
| `query.rs` | 邻接索引 + 遍历（search、callers、callees、impact、path、file、module、symbol）。`find_nodes()` 优先级：精确 id > 精确 qualified_name > 短名 > 后缀 |
| `semantic.rs` | rust-analyzer LSP 增强，自动检测 PATH（可通过 `--no-semantic` 关闭） |
| `mir.rs` | rustc MIR 文本解析（`--mir` 标志） |
| `ai.rs` | AI 导航命令（guide、entries、clusters、quality、health、map） |
| `rag.rs` | 检索：词法搜索 → 嵌入相似度 → 重排 |
| `llm.rs` | LLM 客户端，供 `ask` 命令使用 |
| `config.rs` | 全局 LLM/RAG 配置读写（`~/.config/crabmap/config.json`） |
| `report.rs` | GRAPH_REPORT.md 和 AGENT_GUIDE.md 生成 |
| `health.rs` | 架构风险检测（循环依赖、上帝模块、死代码） |
| `deps.rs` | 模块依赖方向分析 |
| `test_impact.rs` | 静态测试候选发现 |
| `gitintel.rs` | Git 变更频率/归属/共同变更分析（需要 git 仓库） |
| `drift.rs` | 与 git base 的图差异对比 |
| `repo_map.rs` | 基于 token 预算的仓库地图 |
| `export.rs` | DOT/Mermaid/JSON 导出 |
| `term.rs` | ANSI 终端颜色（红、绿、黄、青、粗体），带 TTY 检测 |
| `web.rs` | 嵌入式 HTTP 浏览器，通过 `include_str!` 提供静态 web 资源 |

### Web UI（`web/`）

深色主题，微内核架构。15 个模块化文件通过 `include_str!` 提供：

| 文件 | 用途 |
|---|---|
| `index.html` | HTML 骨架，侧边栏（搜索、边筛选标签、指标），画布区（图 SVG、边图例、状态栏、缩放控件、详情抽屉） |
| `styles/base.css` | CSS 变量、重置、排版 |
| `styles/layout.css` | 网格、面板、侧边栏（260px）、抽屉 |
| `styles/components.css` | 按钮、卡片、标签、输入框、边筛选标签 |
| `styles/graph.css` | SVG 节点/边、边标签、边图例 |
| `src/core.js` | 微内核：状态存储 + 事件总线 |
| `src/utils.js` | 工具函数，`nodeColor()`、`edgeColor()`、`edgeLegend()` |
| `src/api.js` | HTTP 客户端（`/api/status`、`/api/graph`、`/api/search`、`/api/symbol`、`/api/callees`、`/api/callers`、`/api/impact`、`/api/reindex`） |
| `src/graph-layout.js` | 种子位置 + 力导向松弛 |
| `src/graph-render.js` | SVG 渲染：按类型着色边、裁剪线端点、箭头标记、按类型边图例、基于度数的节点半径 |
| `src/graph-interact.js` | 拖拽/缩放/选择 |
| `src/sidebar.js` | 搜索结果、边筛选标签（中文标签 + 彩色圆点，localStorage）、指标 |
| `src/details.js` | 详情抽屉 + 文件符号列表 |
| `src/toolbar.js` | 搜索、深度、重建索引、状态、自动选择 `crabmap::run` |
| `src/main.js` | 引导启动 |

## CLI 结构

```
crabmap
├── index [项目路径]     # 构建图（--all 用于 workspace、--no-tests、--no-semantic、--mir）
├── serve [项目路径]     # 启动 HTTP 浏览器 + API（--port、--watch）
├── query               # 读操作
│   ├── stats           #   按类型、来源、确定性的节点/边计数
│   ├── summary         #   按度数排序的热点符号
│   ├── symbols         #   所有符号（--kind、--limit）
│   ├── symbol <名字>   #   单个符号（多个匹配时显示 ambiguous）
│   ├── file <路径>     #   文件中声明的符号
│   ├── module <名字>   #   模块中声明的符号
│   ├── callees <ID>    #   下游调用图（--depth）
│   ├── callers <ID>    #   上游调用图（--depth）
│   ├── impact <ID>     #   完整依赖影响（--depth）
│   ├── search <查询>   #   跨名称/签名/文档的文本搜索
│   ├── path <FROM> <TO>#   两个符号间的最短路径
│   └── export          #   DOT/Mermaid/JSON 导出（--format）
├── nav                 # AI 导向导航
│   ├── guide           #   入口点 + 调用链
│   ├── entries         #   检测到的入口点
│   ├── clusters        #   按文件的特征聚类
│   ├── quality         #   图质量评分 + 建议
│   ├── health          #   循环、上帝模块、死代码
│   └── map             #   基于 token 预算的仓库概览
├── analyze             # 静态分析（部分需要 git）
│   ├── deps            #   模块依赖矩阵
│   ├── fanout          #   文件级扇入/扇出
│   ├── tests           #   测试影响候选
│   ├── hotspots        #   Git 变更热点
│   └── diff            #   与 git base 的图差异
└── config              # LLM/RAG API 密钥和模型设置
```

## 关键设计决策

### 符号解析
- `find_nodes()` 优先级：精确 id 匹配 > 精确 qualified_name > 短名匹配 > 后缀匹配。
- 短名模糊时返回所有匹配；遍历命令（callees/callers/impact/path）报 "ambiguous" 并列出所有 qualified_name。
- "Not found" 错误包含模糊建议（编辑距离 ≤ 3 的最接近匹配）。

### 进度报告
- 索引先收集全部文件，然后用进度条（基于 `indicatif`）逐步处理。
- 进度条输出到 stderr，stderr 非 TTY 时自动隐藏。
- 完成后输出摘要行：✓ indexed N nodes, M edges in F files。

### 终端输出
- 通过 ANSI 码（`term` 模块）彩色输出：红=错误、绿=成功、黄=警告、青=URL。
- stderr 被管道时自动关闭颜色（通过 `IsTerminal` 检测）。

### 错误信息
- 符号/文件/模块 "not found" 错误包含基于编辑距离的建议。
- 格式：`symbol 'inde_project' not found\nDid you mean?\n  • index_project`

### 边着色（Web UI）
- 边按 `kind` 着色，而非来源：`calls`=蓝、`declares`=琥珀、`uses_type`=紫、`contains`=翠绿、`imports`=青、`has_method`=粉、`returns`=橙、`module_file`=灰、`implements`=天蓝、`possible_dispatch`=红。
- `possible` 边：虚线。`rust_analyzer`/`mir` 边：发光效果 + 加粗。
- 箭头标记：12×10px，白色描边，线端点裁剪到节点半径使箭头位于圆圈外侧。
- 边种类筛选：中文标签 + 彩色圆点切换按钮，localStorage 存储，默认仅 `calls` 激活。

### 布局（Web UI）
- 力导向：排斥常数 2800，理想边长 210（调用）/ 155（声明），小图 150 次迭代。
- 节点半径：7–18px 与 sqrt(degree) 成正比。
- 中心节点固定；其他节点受重力拉向中心（邻域模式 0.0012）。

### 分析器调用解析
- 函数调用：优先同模块解析再跨模块。
- 方法调用：仅解析到 trait impl 方法，不匹配独立函数。

### 语义增强
- rust-analyzer 自动检测 PATH。默认启用；通过 `--no-semantic` 关闭。
- `--semantic-limit` 控制最大扫描符号数（默认：200）。

## 关键约定

- **默认图输出**：`<project>/.crabmap/crabmap.json.gz`。使用 `--output` 覆盖。
- **测试项目**：`tests/fixtures/sample/` — 所有集成测试使用的极简 Rust crate。
- **测试模式**：测试通过 `std::process::Command` 调用编译后的二进制，索引测试项目，对 JSON 响应进行断言。
- 所有 CLI 输出均为 JSON。
- 边溯源：`source`（ast/rust_analyzer/mir/inferred）× `certainty`（definite/confirmed/inferred/possible）。

## 测试

- 6 个单元测试位于 `src/query.rs`（模糊符号解析、文件/模块/符号查询、路径失败）。
- 8 个集成测试位于 `tests/cli.rs`（索引、查询、语义、MIR、`--all`、profiles、自举）。
- 全部 14 个测试通过。运行：`cargo test`。

## 已知限制

- `hotspots` 和 `diff` 需要 git 仓库；非 git 仓库优雅失败。
- MIR 模式轻量测试；需要 nightly rustc 且 `RUSTC_BOOTSTRAP=1`。
- `--watch` 热重载未充分测试。
- 200+ 可见节点时布局可能变密。
- 大型项目（10k+ 节点）未测试 — 索引性能和图形渲染可能下降。
- proc macro 和复杂泛型可能产生不完整的调用边。

## 添加新的 CLI 命令

1. 确定命令所属分组（`query`、`nav`、`analyze` 或顶层）。
2. 在 `cli.rs` 的子命令枚举中添加变体 + 参数结构体。
3. 在 `main.rs` 的分组分发块中添加 match arm。
4. 在现有或新模块中实现。
5. 在 `tests/cli.rs` 中添加测试，使用嵌套格式：`"query" "search"`、`"nav" "guide"`、`"analyze" "deps"`。

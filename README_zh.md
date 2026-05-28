# 🦀 Crabmap

<p align="center">
  <strong>Rust 代码卫星地图 — 索引、查询、导航你的代码库</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-1.85%2B-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License">
  <img src="https://img.shields.io/crates/v/crabmap.svg" alt="crates.io">
  <img src="https://img.shields.io/badge/status-active-brightgreen.svg" alt="Status">
  <a href="README.md">🇬🇧 English</a> · <a href="AGENTS_zh.md">📖 开发文档</a>
</p>

---

**Crabmap** 把任意 Rust 项目变成一份持久的、可查询的知识图谱。把它扔给 AI——AI 不需要逐文件阅读，就能理解整个代码库。

打个比方：**LSP 是显微镜**，让你看清一个细胞；**Crabmap 是卫星地图**，让你一眼看清整座城市。

---

## ✨ 为什么不用 LSP？

| LSP / rust-analyzer | Crabmap |
|:---|:---|
| 一次看一个符号 | **一张图看清整个项目** |
| 必须开着 IDE | **离线便携的 JSON 文件** |
| 没有"项目概览" | **`nav map`** — 8000 token 的架构摘要 |
| 「查找引用」给平铺列表 | **`query impact`** — 完整依赖传播链 |
| 不做架构诊断 | **`analyze health`** — 循环依赖、上帝模块、死代码 |
| 给人用的（悬浮、点击） | **给 AI 用的** — 为 LLM 上下文窗口设计 |

---

## 🚀 快速开始

```bash
# 安装
cargo install crabmap

# 给项目建索引
crabmap index /path/to/rust/project
# ✓ indexed 9089 nodes, 14355 edges in 168 files

# 导出一份 AI 可读的架构地图
crabmap nav map

# 搜索任何符号
crabmap query search "handle_connection"

# 在浏览器里交互式探索
crabmap serve
```

---

## 📦 命令一览

### `crabmap index` — 构建图谱

```bash
crabmap index .                           # 索引当前项目
crabmap index --all .                     # 发现并索引目录下所有 Cargo 项目
crabmap index --no-tests                  # 跳过测试文件
crabmap index --output custom.json.gz     # 自定义输出路径（gzip 压缩）
```

### `crabmap query` — 查询图谱

```bash
crabmap query stats                       # 节点/边统计
crabmap query search "config"             # 模糊文本搜索
crabmap query symbol main                 # 查看某个符号的详情
crabmap query callees main --depth 3      # main 调用了谁？
crabmap query callers load_config         # 谁调用了它？
crabmap query impact Runtime --depth 2    # 完整依赖影响链
crabmap query path main load_config       # 两个符号间的最短调用路径
```

### `crabmap nav` — 给 AI 的导航

```bash
crabmap nav map           # token 预算内的项目概览（给 LLM 用的）
crabmap nav guide         # 入口点 + 调用链
crabmap nav clusters      # 按文件的特征聚类
crabmap nav quality       # 图谱置信度评分
crabmap nav health        # 循环依赖、上帝模块、死代码检测
```

### `crabmap analyze` — 静态分析

```bash
crabmap analyze deps      # 模块依赖矩阵
crabmap analyze fanout    # 文件级扇入/扇出
crabmap analyze tests     # 测试影响候选
crabmap analyze hotspots  # Git 变更热点
crabmap analyze diff      # 与 git base 的图谱差异
```

### `crabmap serve` — Web 可视化

```bash
crabmap serve                         # 索引 + 启动服务
crabmap serve --graph graph.json.gz   # 加载预先建好的图谱
crabmap serve --watch                 # 文件变更自动重建索引
```

### `crabmap config` — 配置 API 密钥（LLM 功能）

```bash
crabmap config --api-key sk-... --model gpt-4
```

---

## 🌐 Web 界面

运行 `crabmap serve` 后打开 `http://127.0.0.1:7878`：

- **图谱可视化** — 力导向布局，节点和边颜色按类型区分
- **交互探索** — 点击节点展开关联图，拖拽调整位置
- **关系筛选** — 中文标签切换按钮，按需开关调用/声明/类型使用等关系
- **详情面板** — 查看符号、文件、边的详细信息
- **深色主题** — 完整暗色 UI

---

## 🧪 实测数据

| 项目 | 节点 | 边 | Warnings | 质量分 |
|:---|--:|--:|:--:|:--:|
| crabmap（自举） | 899 | 1,676 | 0 | 99 |
| ripgrep | 9,089 | 14,355 | 0 | 96 |
| tokio | 14,176 | 28,831 | 0 | 98 |

三个项目全部以 **零 warning** 通过索引。

---

## 🔧 工作原理

1. **`cargo metadata`** → 发现包、目标、源文件
2. **`syn` AST 遍历** → 提取结构体、枚举、函数、方法、impl、宏……
3. **调用解析** → 优先同模块匹配，方法调用仅匹配 trait impl
4. **rust-analyzer 增强**（可选）→ LSP 调用层级确认边
5. **MIR 降级**（可选）→ rustc MIR 分派点分析
6. **图谱持久化** → gzip 压缩 JSON（比原始 JSON 小 14 倍）

---

## 📄 图谱格式

输出为单文件 JSON（默认 gzip 压缩）：

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

| 边类型 | 含义 |
|:---|:---|
| `calls` | 函数/方法调用 |
| `declares` | 模块声明符号 |
| `uses_type` | 类型引用 |
| `contains` | 文件包含模块 |
| `imports` | `use` 导入 |
| `has_method` | impl 块的方法 |
| `implements` | trait 实现 |
| `returns` | 返回类型 |
| `module_file` | 文件 ↔ 模块映射 |

---

## 🛠 从源码构建

```bash
git clone https://github.com/trtyr/crabmap.git
cd crabmap
cargo build --release
./target/release/crabmap --version
# crabmap 0.1.2 (abc1234 2026-05-21)
```

需要 Rust ≥ 1.85（edition 2024）。

---

## 📁 项目结构

```
src/
├── main.rs          # CLI 入口 & 命令分发
├── cli.rs           # clap 参数定义
├── analyzer.rs      # AST 索引器（syn 遍历）
├── query.rs         # 图遍历 & 搜索
├── model.rs         # 核心数据模型
├── store.rs         # Gzip JSON 读写
├── web.rs           # 嵌入式 HTTP 服务器
├── semantic.rs      # rust-analyzer 语义增强
├── mir.rs           # MIR 降级分析
├── ai.rs            # AI 导航命令
├── config.rs        # 全局配置 (~/.config/crabmap/)
├── term.rs          # ANSI 终端颜色
├── health.rs        # 架构风险检测
└── …

web/
├── index.html
├── styles/          # CSS（深色主题）
└── src/             # JS（微内核架构）

skills/
└── crabmap.md     # AI Agent 使用指南
```

---

## 📝 许可证

MIT

---

<p align="center">
  <sub>为 AI 而生 · 用 🦀 Rust 构建</sub>
</p>

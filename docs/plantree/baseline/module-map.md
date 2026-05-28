# Module Map

## 项目概述

Crabmap 是一个 Rust 代码知识图谱工具，用于 AI 导航。它将 Rust 项目索引为结构化的调用图，支持查询、分析和可视化。

## 模块结构

### 核心模块

| 模块 | 行数 | 符号数 | 职责 |
|------|------|--------|------|
| `analyzer/` | ~1217 | 92 | AST 索引器，解析 Rust 源码构建图 |
| `analyzer/index.rs` | 619 | 7 | 核心索引逻辑 |
| `analyzer/builder.rs` | ~250 | 15 | Builder 结构体和方法 |
| `analyzer/helpers.rs` | ~150 | 12 | 工具函数 |
| `analyzer/visitors.rs` | ~80 | 5 | AST 访问器实现 |
| `analyzer/types.rs` | 45 | 4 | 类型定义 |
| `model.rs` | 297 | 139 | 核心数据模型：CodeGraph, Node, Edge |
| `query/` | ~836 | 41 | 图查询和遍历 |
| `query/commands.rs` | ~180 | 8 | 公共 API |
| `query/index.rs` | ~45 | 4 | 邻接索引 |
| `query/find.rs` | ~80 | 5 | 符号解析 |
| `query/traversal.rs` | ~110 | 4 | 图遍历 |
| `query/ranking.rs` | ~80 | 2 | 排序 |
| `main.rs` | 411 | 19 | CLI 入口和命令分发 |

### 功能模块

| 模块 | 行数 | 符号数 | 职责 |
|------|------|--------|------|
| `web.rs` | 510 | 70 | HTTP 服务器和 Web UI |
| `ai.rs` | 559 | 34 | AI 导航命令 |
| `semantic.rs` | 638 | 40 | rust-analyzer 语义增强 |
| `rag.rs` | 386 | 47 | 检索增强生成 |
| `llm.rs` | 369 | 28 | LLM 客户端 |
| `config.rs` | 198 | 15 | 全局配置管理 |

### 分析模块

| 模块 | 行数 | 符号数 | 职责 |
|------|------|--------|------|
| `health.rs` | 264 | 18 | 架构风险检测 |
| `deps.rs` | 128 | 8 | 模块依赖分析 |
| `drift.rs` | 128 | 8 | 图谱 diff 分析 |
| `gitintel.rs` | 153 | 10 | Git 情报分析 |
| `test_impact.rs` | 92 | 5 | 测试影响分析 |

### 工具模块

| 模块 | 行数 | 符号数 | 职责 |
|------|------|--------|------|
| `store.rs` | 169 | 10 | 图谱持久化 |
| `export.rs` | 80 | 5 | 导出功能 |
| `term.rs` | 36 | 6 | 终端颜色 |
| `report.rs` | 269 | 15 | 报告生成 |
| `repo_map.rs` | 116 | 8 | 仓库地图 |

## 依赖关系

### 核心依赖链

```
main.rs → cli.rs → 所有模块
main.rs → analyzer.rs → model.rs
main.rs → query.rs → model.rs
main.rs → web.rs → 多个模块
```

### 循环依赖（问题）

1. `ai.rs → web.rs → main.rs → ai.rs`
2. `analyzer.rs → config.rs → web.rs → analyzer.rs`
3. `config.rs → report.rs → main.rs → config.rs`
4. `web.rs → analyzer.rs → web.rs`

### 热点模块

1. `model.rs`：degree=535，被几乎所有模块依赖
2. `analyzer.rs`：degree=559，核心索引器
3. `cli.rs`：degree=464，命令定义
4. `web.rs`：degree=319，Web 服务器

## God Modules

| 模块 | 行数 | 符号数 | 问题 |
|------|------|--------|------|
| `analyzer.rs` | 1217 | 92 | 过大，需要拆分 |
| `query.rs` | 836 | 41 | 过大，需要拆分 |
| `tests/cli.rs` | 655 | 27 | 测试文件过大 |
| `semantic.rs` | 638 | 40 | 过大，需要拆分 |
| `ai.rs` | 559 | 34 | 过大，需要拆分 |
| `web.rs` | 510 | 70 | 过大，需要拆分 |
| `cli.rs` | 334 | 155 | 符号过多 |
| `model.rs` | 297 | 139 | 符号过多 |
| `rag.rs` | 386 | 47 | 过大，需要拆分 |

## 可能的死代码

| 模块 | 符号 | 说明 |
|------|------|------|
| `term.rs` | `red`, `green`, `yellow`, `cyan`, `bold` | 未使用的颜色函数 |
| `config.rs` | `redacted` | 未使用的函数 |
| `model.rs` | `Project`, `as_str` (多个) | 未使用的类型和方法 |
| `cli.rs` | `Cli` | 未使用的结构体 |
| `test_impact.rs` | `changed_targets` | 未使用的函数 |

## 最后更新

2026-05-28
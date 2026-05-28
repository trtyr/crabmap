# Runtime Flows

## 主要数据流

### 1. 索引流程

```
用户输入 → CLI → analyzer.rs → model.rs → store.rs
                ↓
            语法解析 (syn)
                ↓
            图构建 (Builder)
                ↓
            持久化 (gzip JSON)
```

### 2. 查询流程

```
用户输入 → CLI → query.rs → model.rs → 结果
                ↓
            图遍历 (petgraph)
                ↓
            搜索/过滤
                ↓
            JSON 输出
```

### 3. Web 服务流程

```
浏览器 → HTTP → web.rs → query.rs → JSON 响应
                ↓
            静态文件服务
                ↓
            API 端点处理
```

### 4. AI 导航流程

```
用户输入 → CLI → ai.rs → model.rs → JSON 输出
                ↓
            图分析算法
                ↓
            Token 预算优化
```

## 关键数据结构

### CodeGraph（核心）

```rust
struct CodeGraph {
    schema_version: u32,
    project: Project,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    warnings: Vec<String>,
    semantic: Option<SemanticInfo>,
    mir: Option<MirInfo>,
    profiles: Vec<BuildProfile>,
    generated_at_ms: u128,
}
```

### Node（节点）

```rust
struct Node {
    id: String,
    kind: NodeKind,  // Project, Crate, File, Module, Function, etc.
    name: String,
    qualified_name: String,
    file: Option<String>,
    range: Option<Range>,
    visibility: Option<String>,
    signature: Option<String>,
    docs: Option<String>,
    metrics: BTreeMap<String, usize>,
}
```

### Edge（边）

```rust
struct Edge {
    from: String,
    to: String,
    kind: EdgeKind,  // Calls, Declares, Imports, etc.
    label: Option<String>,
    evidence: Option<Location>,
    weight: usize,
    source: EdgeSource,  // Ast, RustAnalyzer, Mir, Inferred
    certainty: EdgeCertainty,  // Definite, Confirmed, Inferred, Possible
    profiles: Vec<String>,
}
```

## 模块间调用关系

### analyzer.rs 调用链

```
index_project()
  → index_package()
    → index_target()
      → index_file()
        → index_item()
          → index_impl()
          → collect_function_edges()
          → type_use()
```

### query.rs 调用链

```
search()
  → find_nodes()
  → fuzzy_match()

callees()
  → traverse()
  → filter_edges()

callers()
  → reverse_traverse()
  → filter_edges()

impact()
  → full_dependency_analysis()
```

### web.rs 调用链

```
serve()
  → handle_request()
    → serve_static()
    → serve_api()
      → query_stats()
      → query_search()
      → query_symbol()
```

## 性能关键点

### 1. 索引性能

- **瓶颈**：syn AST 解析
- **优化**：并行文件处理，增量索引

### 2. 查询性能

- **瓶颈**：图遍历算法
- **优化**：预计算索引，缓存热点查询

### 3. Web 性能

- **瓶颈**：JSON 序列化
- **优化**：流式响应，压缩传输

## 内存使用

### 主要内存消耗

1. **CodeGraph**：节点和边的存储
2. **HashMap 索引**：快速查找
3. **字符串存储**：qualified_name, file 路径

### 优化方向

1. 字符串 interning
2. 节点池分配
3. 延迟加载

## 最后更新

2026-05-28
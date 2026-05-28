# Storage and State

## 存储架构

### 1. 图谱存储

**格式**：gzip 压缩的 JSON 文件
**位置**：`.crabmap/crabmap.json.gz`
**大小**：当前 55.8KB

```json
{
  "schema_version": 2,
  "project": { "root": ".", "packages": [...] },
  "nodes": [...],
  "edges": [...],
  "warnings": [...],
  "semantic": {...},
  "mir": {...},
  "profiles": [...],
  "generated_at_ms": 1234567890
}
```

### 2. 配置存储

**格式**：JSON 文件
**位置**：`~/.config/crabmap/config.json`

```json
{
  "api_key": "sk-...",
  "model": "gpt-4",
  "embedding_key": "...",
  "embedding_model": "text-embedding-3-small"
}
```

### 3. 缓存存储

**格式**：内存缓存
**位置**：运行时内存
**生命周期**：进程生命周期

## 状态管理

### 1. 图谱状态

```rust
struct CodeGraph {
    // 持久化状态
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    
    // 运行时索引
    nodes_by_id: HashMap<String, usize>,
    nodes_by_qname: HashMap<String, usize>,
    edges_by_from: HashMap<String, Vec<usize>>,
    edges_by_to: HashMap<String, Vec<usize>>,
}
```

### 2. Web 服务器状态

```rust
struct ServeConfig {
    // 配置状态
    project: PathBuf,
    host: String,
    port: u16,
    
    // 运行时状态
    graph: Arc<Mutex<CodeGraph>>,
    last_modified: SystemTime,
}
```

### 3. CLI 状态

```rust
struct Cli {
    // 命令行参数
    command: Command,
    verbose: bool,
    color: bool,
}
```

## 数据流

### 写入流

```
用户操作 → CLI 解析 → 数据处理 → 图构建 → JSON 序列化 → gzip 压缩 → 文件写入
```

### 读取流

```
文件读取 → gzip 解压 → JSON 反序列化 → 图构建 → 索引构建 → 查询处理
```

### 更新流

```
文件变化 → 重新索引 → 增量更新 → 状态同步
```

## 并发安全

### 1. Web 服务器

- 使用 `Arc<Mutex<CodeGraph>>` 共享图谱
- 每个请求独立处理
- 无全局可变状态

### 2. CLI 工具

- 单线程执行
- 无并发冲突
- 原子文件操作

## 持久化策略

### 1. 立即持久化

- 索引完成后立即保存
- 配置修改后立即保存
- 确保数据不丢失

### 2. 增量更新

- 监控文件变化
- 局部重新索引
- 合并到现有图谱

### 3. 版本管理

- Schema 版本控制
- 向后兼容性
- 迁移脚本

## 备份和恢复

### 1. 自动备份

- 保存前备份旧文件
- 保留最近 N 个版本
- 清理过期备份

### 2. 手动恢复

- 从备份恢复
- 重新索引
- 配置重置

## 最后更新

2026-05-28
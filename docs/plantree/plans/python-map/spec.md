# Python Map — Complete Specification

> Python 代码知识图谱工具，与 crabmap (Rust) 功能对齐。

## 1. 项目定位

**名称**：pymap（暂定）

**定位**：Python 版 crabmap — 索引 Python 项目为调用图，支持搜索、查询、导航、分析。

**目标用户**：AI agent（主要）、人类开发者（次要）

**核心价值**：
- 不读源码就能理解项目结构
- 不 grep 就能追踪调用链
- 改代码前评估影响范围
- 重构前规划安全顺序

---

## 2. 数据模型

### 2.1 CodeGraph（顶层）

```python
@dataclass
class CodeGraph:
    schema_version: int          # 图格式版本
    project: Project             # 项目元信息
    nodes: list[Node]            # 所有节点
    edges: list[Edge]            # 所有边
    warnings: list[str]          # 索引警告
    generated_at: datetime       # 生成时间
```

### 2.2 Project

```python
@dataclass
class Project:
    root: str                    # 项目根目录
    packages: list[Package]      # 包列表（支持 monorepo）
    python_version: str          # Python 版本要求
    build_system: str            # 构建系统 (pip/poetry/uv/pdm/hatch)
```

### 2.3 Package

```python
@dataclass
class Package:
    name: str                    # 包名
    manifest_path: str           # pyproject.toml / setup.py 路径
    src_path: str                # 源码根目录
    dependencies: list[str]      # 直接依赖
```

### 2.4 Node（节点）

```python
@dataclass
class Node:
    id: str                      # 唯一 ID (如 "pkg.mod.Class.method")
    kind: NodeKind               # 节点类型
    name: str                    # 短名称
    qualified_name: str          # 全限定名
    file: str | None             # 文件路径
    range: Range | None          # 行范围
    visibility: str | None       # 可见性 (public/private/protected)
    signature: str | None        # 签名
    docs: str | None             # 文档字符串
    decorators: list[str]        # 装饰器列表
    type_annotations: dict       # 类型注解信息
    is_async: bool               # 是否异步
    metrics: dict[str, int]      # 度量指标
```

### 2.5 NodeKind（节点类型）

```python
class NodeKind(Enum):
    # 项目级
    PROJECT = "project"
    PACKAGE = "package"          # 对应 Rust 的 Crate
    FILE = "file"
    MODULE = "module"

    # 类型系统
    CLASS = "class"              # 对应 Rust 的 Struct
    ENUM = "enum"
    NAMED_TUPLE = "named_tuple"
    DATA_CLASS = "data_class"    # Python 特有
    TYPE_ALIAS = "type_alias"
    PROTOCOL = "protocol"        # Python 特有 (typing.Protocol)
    ABC = "abc"                  # Python 特有 (abc.ABC)

    # 函数/方法
    FUNCTION = "function"
    METHOD = "method"
    CLASS_METHOD = "class_method"      # Python 特有
    STATIC_METHOD = "static_method"    # Python 特有
    PROPERTY = "property"              # Python 特有
    ASYNC_FUNCTION = "async_function"  # Python 特有
    ASYNC_METHOD = "async_method"      # Python 特有
    GENERATOR = "generator"            # Python 特有
    CONSTRUCTOR = "constructor"        # __init__
    DUNDER = "dunder"                  # Python 特有 (__str__, __eq__, etc.)

    # 变量/属性
    VARIABLE = "variable"
    CLASS_VARIABLE = "class_variable"
    INSTANCE_VARIABLE = "instance_variable"
    CONSTANT = "constant"              # UPPER_CASE 变量

    # 装饰器
    DECORATOR = "decorator"            # Python 特有

    # 导入
    IMPORT = "import"                  # Python 特有

    # 其他
    UNKNOWN = "unknown"
```

**与 crabmap 的映射**：

| crabmap NodeKind | pymap NodeKind | 说明 |
|------------------|----------------|------|
| Crate | Package | 包/库 |
| Struct | Class / DataClass / NamedTuple | 类型 |
| Trait | Protocol / ABC | 接口抽象 |
| Impl | (隐含在类方法中) | Python 无单独 impl 块 |
| Function | Function / AsyncFunction | 函数 |
| Method | Method / ClassMethod / StaticMethod | 方法 |
| Constructor | Constructor (__init__) | 构造器 |
| Macro | Decorator | 元编程 |
| Const / Static | Constant | 常量 |
| (无) | Dunder | Python 特有 |
| (无) | Generator | Python 特有 |
| (无) | Import | Python 特有 |

### 2.6 Edge（边）

```python
@dataclass
class Edge:
    from_id: str                 # 起点节点 ID
    to_id: str                   # 终点节点 ID
    kind: EdgeKind               # 边类型
    label: str | None            # 标签
    evidence: Location | None    # 证据位置
    weight: int                  # 权重（调用次数）
    source: EdgeSource           # 来源
    certainty: EdgeCertainty     # 确定性
```

### 2.7 EdgeKind（边类型）

```python
class EdgeKind(Enum):
    # 结构关系
    CONTAINS = "contains"            # 包含（模块包含类）
    DECLARES = "declares"            # 声明（文件声明函数）
    MODULE_FILE = "module_file"      # 模块对应文件

    # 调用关系
    CALLS = "calls"                  # 函数调用
    AWAIT_CALLS = "await_calls"      # Python 特有：await 调用

    # 类型关系
    INHERITS = "inherits"            # Python 特有：类继承
    IMPLEMENTS = "implements"        # 实现协议/ABC
    USES_TYPE = "uses_type"          # 类型引用
    RETURNS = "returns"              # 返回类型
    DECORATES = "decorates"          # Python 特有：装饰器应用

    # 导入关系
    IMPORTS = "imports"              # 导入
    FROM_IMPORTS = "from_imports"    # Python 特有：from X import Y

    # 特殊关系
    HAS_METHOD = "has_method"        # 类拥有方法
    OVERRIDES = "overrides"          # Python 特有：方法重写
    MIXIN = "mixin"                  # Python 特有：mixin 关系
    DESCRIBES = "describes"          # Python 特有：property 描述符
```

**与 crabmap 的映射**：

| crabmap EdgeKind | pymap EdgeKind | 说明 |
|------------------|----------------|------|
| Contains | Contains | 包含 |
| Declares | Declares | 声明 |
| Imports | Imports / FromImports | 导入 |
| Calls | Calls / AwaitCalls | 调用 |
| Implements | Implements | 实现 |
| HasMethod | HasMethod | 拥有方法 |
| UsesType | UsesType | 类型引用 |
| Returns | Returns | 返回 |
| ModuleFile | ModuleFile | 模块文件 |
| (无) | Inherits | Python 特有 |
| (无) | Decorates | Python 特有 |
| (无) | Overrides | Python 特有 |
| (无) | Mixin | Python 特有 |
| (无) | Describes | Python 特有 |

### 2.8 EdgeSource（边来源）

```python
class EdgeSource(Enum):
    AST = "ast"                  # AST 解析
    TYPE_CHECKER = "type_checker"  # 类型检查器 (pyright/mypy)
    INFERRED = "inferred"        # 推断
```

### 2.9 EdgeCertainty（边确定性）

```python
class EdgeCertainty(Enum):
    DEFINITE = "definite"        # 确定
    CONFIRMED = "confirmed"      # 确认（类型检查器验证）
    INFERRED = "inferred"        # 推断
    POSSIBLE = "possible"        # 可能（动态特性）
```

---

## 3. CLI 命令

### 3.1 命令结构

```
pymap
├── index [PROJECT]            # 构建图
├── serve [PROJECT]            # 启动 HTTP 查看器
├── query                      # 读操作
│   ├── stats                  #   节点/边统计
│   ├── summary                #   热点符号排名
│   ├── symbols                #   所有符号 (--kind, --limit)
│   ├── symbol <NAME>          #   单个符号详情
│   ├── file <PATH>            #   文件中的符号
│   ├── module <NAME>          #   模块中的符号
│   ├── callees <ID>           #   下游调用图
│   ├── callers <ID>           #   上游调用图
│   ├── impact <ID>            #   完整依赖影响
│   ├── risk <ID>              #   风险评估（impact + 测试覆盖）
│   ├── search <QUERY>         #   文本搜索
│   ├── find <NAME>            #   查找符号
│   ├── similar <NAME>         #   结构相似符号
│   ├── path <FROM> <TO>       #   最短调用路径
│   └── export                 #   导出 (--format)
├── nav                        # AI 导航
│   ├── guide                  #   入口点 + 调用链
│   ├── entries                #   检测到的入口点
│   ├── clusters               #   功能聚类
│   ├── quality                #   图质量评分
│   ├── health                 #   架构健康（循环、上帝模块、死代码）
│   └── map                    #   Token 预算化的仓库概览
├── analyze                    # 静态分析
│   ├── deps                   #   模块依赖矩阵
│   ├── fanout                 #   文件级扇入/扇出
│   ├── tests                  #   测试影响分析
│   ├── hotspots               #   Git 热点
│   ├── diff                   #   图 diff vs git base
│   ├── refactor-order         #   重构顺序（拓扑排序）
│   ├── type-coverage          #   Python 特有：类型注解覆盖率
│   ├── async-map              #   Python 特有：异步函数地图
│   └── decorator-usage        #   Python 特有：装饰器使用分析
└── config                     # 配置
```

### 3.2 详细命令说明

#### `pymap index`

```bash
pymap index .                          # 索引当前项目
pymap index --all .                    # 递归查找所有 pyproject.toml
pymap index --all --max-depth 3 .      # 限制递归深度
pymap index --no-tests                 # 跳过测试文件
pymap index --output custom.json.gz    # 自定义输出路径
pymap index --output-dir .pymap/ .     # 共享输出目录
pymap index --no-type-checker          # 跳过类型检查器集成
pymap index --type-checker pyright     # 指定类型检查器 (pyright/mypy)
pymap index --include-stubs            # 包含 .pyi 存根文件
```

**Python 特有选项**：
- `--no-type-checker`：跳过类型检查器集成（默认启用 pyright）
- `--type-checker <TOOL>`：选择类型检查器 (pyright/mypy/none)
- `--include-stubs`：包含 .pyi 类型存根文件
- `--exclude-patterns <PATTERNS>`：排除 glob 模式

#### `pymap query`

所有 query 命令支持 `--graph <FILE>`、`--depth`、`--limit`。

**统计与概览**：
```bash
pymap query stats                      # 节点/边/文件计数
pymap query summary                    # 项目摘要
pymap query symbols                    # 列出所有符号
pymap query symbols --kind function    # 仅函数
pymap query symbols --dead             # 无调用者的函数
pymap query symbols --no-docs          # 无文档字符串的符号
pymap query symbols --no-type-hints    # Python 特有：无类型注解
pymap query symbols --min-degree 20    # 高连通性符号
pymap query symbols --visibility public --kind class --min-callers 3
```

**符号过滤标志**：

| 标志 | 效果 |
|------|------|
| `--kind` | 按 NodeKind 过滤 |
| `--visibility` | 按可见性过滤 (public/private/protected) |
| `--no-docs` | 无文档字符串的符号 |
| `--no-type-hints` | Python 特有：无类型注解 |
| `--dead` | 零入边的函数/方法/类 |
| `--test-only` | 所有入边来自测试文件 |
| `--async-only` | Python 特有：仅异步函数 |
| `--min-degree` / `--max-degree` | 总边数范围 |
| `--min-callers` / `--max-callers` | 唯一调用者数范围 |
| `--decorator` | Python 特有：按装饰器过滤 |

**符号详情**：
```bash
pymap query symbol <NAME>              # 详细符号信息
pymap query symbol <NAME> --no-source  # 不含源码
pymap query file <PATH>                # 文件中的符号
pymap query module <NAME>              # 模块中的符号
```

**调用追踪**：
```bash
pymap query callees <ID> --depth 3     # 下游调用图
pymap query callers <ID> --depth 3     # 上游调用图
pymap query impact <ID> --depth 2      # 完整依赖影响
pymap query risk <ID>                  # 风险评估
pymap query path <FROM> <TO>           # 最短调用路径
```

**搜索与导出**：
```bash
pymap query search "handle_request"    # 模糊文本搜索
pymap query find "handle_request"      # 查找符号
pymap query find <NAME> --mode similar # 结构相似符号
pymap query export --format json       # 导出 JSON
pymap query export --format dot        # 导出 Graphviz DOT
pymap query export --format mermaid    # 导出 Mermaid
```

#### `pymap nav`

```bash
pymap nav map                          # 紧凑架构摘要 (~8000 tokens)
pymap nav map --full                   # 完整地图（入口点 + 聚类）
pymap nav guide                        # 入口点 + 调用链
pymap nav entries                      # 检测到的入口点
pymap nav clusters                     # 功能聚类
pymap nav quality                      # 图质量评分
pymap nav health                       # 架构健康检查
```

#### `pymap analyze`

```bash
pymap analyze deps                     # 模块依赖矩阵
pymap analyze fanout                   # 文件级扇入/扇出
pymap analyze tests                    # 测试影响分析
pymap analyze hotspots                 # Git 热点
pymap analyze diff                     # 图 diff vs git base
pymap analyze refactor-order A B C     # 重构顺序（拓扑排序）

# Python 特有分析
pymap analyze type-coverage            # 类型注解覆盖率
pymap analyze async-map                # 异步函数地图
pymap analyze decorator-usage          # 装饰器使用分析
```

#### `pymap serve`

```bash
pymap serve                            # 启动 HTTP 查看器 (127.0.0.1:7878)
pymap serve --port 3000                # 自定义端口
pymap serve --watch                    # 文件变更自动重建
```

#### `pymap config`

```bash
pymap config --api-key "sk-..." --model "gpt-4o"
pymap config --embedding-key "..." --embedding-model "text-embedding-3-small"
```

---

## 4. 索引器（Analyzer）

### 4.1 解析策略

**三层解析**：

1. **AST 层**（必需）：Python `ast` 模块
   - 解析所有 .py 文件
   - 提取函数、类、方法、装饰器、导入、类型注解
   - 速度快，无外部依赖

2. **CST 层**（可选）：`libcst`
   - 保留注释、格式
   - 用于精确源码提取
   - 可选，用于 `--include-comments`

3. **类型检查器层**（可选）：pyright / mypy
   - 类型推断和验证
   - 确认调用关系
   - 对应 crabmap 的 rust-analyzer 集成

### 4.2 索引流程

```
1. 发现阶段
   - 查找所有 .py 文件
   - 解析 pyproject.toml / setup.py
   - 确定包结构

2. AST 解析阶段
   - 逐文件 ast.parse()
   - 提取节点（函数、类、方法、变量、装饰器）
   - 提取边（调用、继承、导入、类型引用）

3. 符号解析阶段
   - 构建全限定名
   - 解析导入关系
   - 解析类继承链
   - 解析装饰器目标

4. 类型检查器集成阶段（可选）
   - 调用 pyright/mypy
   - 确认推断的边
   - 添加类型信息

5. 图构建阶段
   - 合并所有节点和边
   - 去重
   - 计算度量指标
   - 保存为 gzip JSON
```

### 4.3 Python 特有处理

**装饰器**：
- 每个装饰器创建一个 DECORATOR 节点
- 被装饰的符号通过 DECORATES 边连接
- 识别常见装饰器模式：@property, @classmethod, @staticmethod, @dataclass, @app.route, @pytest.fixture

**类型注解**：
- 解析函数签名中的类型注解
- 解析变量类型注解
- 通过 USES_TYPE 边连接到类型节点
- 计算类型覆盖率

**异步**：
- 识别 async def
- 创建 ASYNC_FUNCTION / ASYNC_METHOD 节点
- 通过 AWAIT_CALLS 边追踪 await 调用
- 区分同步和异步调用路径

**dunder 方法**：
- 识别 __init__, __str__, __repr__, __eq__, __hash__, __len__, __getitem__, __setitem__, __delitem__, __iter__, __next__, __enter__, __exit__, __call__, __getattr__, __setattr__, __delattr__
- 创建 DUNDER 节点
- 标记特殊行为（上下文管理器、迭代器、可调用等）

**导入解析**：
- 处理 import X, from X import Y, from X import Y as Z
- 处理相对导入 (from . import X, from ..package import Y)
- 处理条件导入 (if TYPE_CHECKING)
- 处理延迟导入（函数内 import）

**动态特性处理**：
- `__getattr__` 动态属性 → POSSIBLE 确定性
- `globals()` / `locals()` 动态调用 → POSSIBLE 确定性
- `eval()` / `exec()` → 标记为 UNKNOWN
- `importlib.import_module()` → POSSIBLE 确定性
- `type()` 动态类创建 → 标记为 UNKNOWN

### 4.4 多项目支持

```bash
# 索引 monorepo 下所有项目
pymap index --all --output-dir .pymap/ . --max-depth 3

# 自动发现并合并
pymap query stats    # 聚合所有项目
```

---

## 5. 查询引擎

### 5.1 符号查找

**查找优先级**（与 crabmap 一致）：
1. 精确 ID 匹配
2. 精确 qualified_name 匹配
3. 短名称匹配
4. 后缀匹配

**模糊建议**：找不到时，Levenshtein 距离 ≤ 3 的最近匹配。

**Python 特有**：
- 支持 `self.method` 语法（解析为当前类的方法）
- 支持 `Class.method` 语法
- 支持 `module.function` 语法

### 5.2 调用追踪

**上游追踪**（callers）：
- 谁调用了这个函数？
- 支持深度限制
- 支持结果数量限制

**下游追踪**（callees）：
- 这个函数调用了什么？
- 包含 await 调用
- 支持深度限制

**Python 特有**：
- 区分同步调用和 await 调用
- 追踪装饰器链（装饰器也是调用）
- 追踪 super() 调用

### 5.3 影响分析

```json
{
  "kind": "impact",
  "root": "myapp.utils.parse_config",
  "dependencies": [...],        // 它依赖什么
  "dependents": [...],          // 什么依赖它
  "callers": [...],             // 直接调用者
  "files_affected": [...],      // 受影响的文件
  "call_sites": [...],          // 调用点
  "change_hints": [...],        // 变更建议
  "risk": {
    "score": 12,
    "level": "high",
    "factors": {...}
  }
}
```

### 5.4 风险评估

与 crabmap 的 risk 命令一致，结合影响分析和测试覆盖：

```json
{
  "kind": "risk",
  "symbol": "myapp.utils.parse_config",
  "risk": {
    "score": 12,
    "level": "high",
    "factors": {
      "files_affected": 5,
      "direct_callers": 8,
      "is_public": true,
      "has_method_callers": true,
      "dependency_count": 12
    },
    "recommendation": "High risk change..."
  },
  "impact_summary": {...},
  "test_coverage": {
    "candidate_tests": [...],
    "has_tests": true
  },
  "suggested_commands": [...]
}
```

### 5.5 结构相似

```bash
pymap query find parse_json --mode similar
```

**相似度计算**：
- 参数数量和类型
- 返回类型
- 调用的函数集合
- 被调用的上下文
- 装饰器集合

### 5.6 重构顺序

```bash
pymap analyze refactor-order module_a module_b module_c
```

**算法**：
1. 构建模块间依赖子图
2. Kahn 拓扑排序
3. Tarjan SCC 检测循环
4. 输出安全重构顺序 + 每步风险评估

---

## 6. 导航（AI 导航）

### 6.1 nav map

输出格式与 crabmap 一致，token 预算化（默认 ~8000 tokens）：

```
# Project: myapp
## Structure
- myapp/ (12 files, 89 symbols)
  - __init__.py
  - config.py (8 symbols) — Configuration loading
  - models.py (15 symbols) — Data models
  - ...

## Entry Points
- myapp.cli:main — CLI entry point
- myapp.server:create_app — WSGI/ASGI factory

## Hot Symbols (by degree)
- myapp.models.User (degree: 45)
- myapp.utils.parse_config (degree: 32)
...

## Clusters
- Authentication: auth.py, middleware.py, tokens.py
- Data Models: models.py, schemas.py
...
```

### 6.2 nav health

```json
{
  "kind": "health",
  "score": 85,
  "issues": [
    {"type": "cycle", "modules": ["auth", "models"], "severity": "medium"},
    {"type": "god_module", "module": "utils", "symbols": 150, "severity": "high"},
    {"type": "dead_code", "functions": [...], "severity": "low"}
  ]
}
```

### 6.3 nav quality

```json
{
  "kind": "quality",
  "score": 72,
  "metrics": {
    "edge_confidence": 0.85,
    "coverage": 0.65,
    "documentation": 0.45,
    "type_hints": 0.60
  }
}
```

---

## 7. 分析

### 7.1 analyze deps

模块依赖矩阵 + 重编译影响：

```json
{
  "kind": "deps",
  "items": [
    {"from": "myapp.auth", "to": "myapp.models", "weight": 5},
    {"from": "myapp.api", "to": "myapp.auth", "weight": 8}
  ],
  "recompile_impact": [
    {"module": "myapp.models", "affected": 12}
  ]
}
```

### 7.2 analyze fanout

文件级扇入/扇出：

```json
{
  "kind": "fanout",
  "items": [
    {"file": "models.py", "fanin": 15, "fanout": 3, "total": 18},
    {"file": "utils.py", "fanin": 20, "fanout": 8, "total": 28}
  ]
}
```

### 7.3 analyze tests

测试影响分析：

```json
{
  "kind": "tests",
  "candidate_tests": [
    {"test": "test_parse_config", "file": "tests/test_utils.py", "covers": ["parse_config"]}
  ],
  "targets": ["myapp.utils.parse_config"],
  "note": "Found 3 test candidates"
}
```

### 7.4 analyze hotspots

Git 热点分析：

```json
{
  "kind": "git",
  "hotspots": [
    {"file": "models.py", "commits": 45, "authors": 8},
    {"file": "auth.py", "commits": 32, "authors": 5}
  ],
  "cochange": [
    {"files": ["models.py", "migrations/"], "count": 12}
  ]
}
```

### 7.5 analyze diff

图 diff vs git base：

```json
{
  "kind": "diff",
  "added_edges": ["new_function -> existing_function"],
  "removed_edges": ["old_function -> deleted_function"],
  "changed_files": ["models.py", "utils.py"]
}
```

### 7.6 analyze refactor-order

重构顺序（拓扑排序）：

```json
{
  "kind": "refactor_order",
  "order": [
    {
      "step": 1,
      "symbol": "module_c",
      "risk": {"score": 0, "level": "low"},
      "reason": "No dependencies"
    },
    {
      "step": 2,
      "symbol": "module_b",
      "risk": {"score": 3, "level": "medium"},
      "reason": "Depends on module_c"
    }
  ]
}
```

### 7.7 Python 特有分析

#### analyze type-coverage

```json
{
  "kind": "type_coverage",
  "overall": 0.65,
  "by_module": [
    {"module": "myapp.models", "coverage": 0.90},
    {"module": "myapp.legacy", "coverage": 0.20}
  ],
  "untyped_functions": [...],
  "untyped_parameters": [...]
}
```

#### analyze async-map

```json
{
  "kind": "async_map",
  "async_functions": [...],
  "sync_functions_called_from_async": [...],
  "blocking_calls_in_async": [...],
  "missing_await": [...]
}
```

#### analyze decorator-usage

```json
{
  "kind": "decorator_usage",
  "decorators": [
    {"name": "@app.route", "count": 25, "files": [...]},
    {"name": "@pytest.fixture", "count": 18, "files": [...]}
  ],
  "custom_decorators": [...]
}
```

---

## 8. 存储

### 8.1 格式

与 crabmap 一致：gzip JSON (`.pymap/pymap.json.gz`)。

### 8.2 自动发现

当 `--graph` 未指定且默认文件不存在时，扫描 `.pymap/` 下所有 `*.json.gz` 文件并合并。

### 8.3 序列化

使用 Python `dataclasses` + `json` 模块，gzip 压缩。

---

## 9. Web 查看器

### 9.1 功能

与 crabmap 一致：
- 深色主题
- 力导向布局
- 边类型过滤（中文标签）
- 节点详情面板
- 搜索功能
- 响应式布局

### 9.2 技术栈

- 后端：Python `http.server` 或 `uvicorn` + `starlette`
- 前端：嵌入式 HTML/JS/CSS（与 crabmap 相同，通过 `include` 嵌入）
- 图渲染：D3.js（与 crabmap 相同）

### 9.3 API 端点

```
GET /                           # 前端页面
GET /api/graph                  # 图数据
GET /api/stats                  # 统计
GET /api/search?q=<query>       # 搜索
GET /api/symbol/<name>          # 符号详情
GET /api/trace/<name>?dir=up    # 调用追踪
GET /api/impact/<name>          # 影响分析
```

---

## 10. 配置

### 10.1 配置路径

`~/.config/pymap/config.json`（与 crabmap 一致）

### 10.2 配置内容

```json
{
  "api_key": "sk-...",
  "model": "gpt-4o",
  "api_url": "https://api.openai.com/v1",
  "embedding_key": "...",
  "embedding_model": "text-embedding-3-small",
  "embedding_url": "...",
  "rerank_key": "...",
  "rerank_model": "...",
  "rerank_url": "...",
  "type_checker": "pyright",
  "exclude_patterns": ["**/test_*", "**/__pycache__/**"],
  "include_stubs": false
}
```

---

## 11. Python 特有功能（crabmap 没有的）

### 11.1 类型注解覆盖率

```bash
pymap analyze type-coverage
```

Python 项目的核心痛点：类型注解覆盖率参差不齐。此命令：
- 统计整体覆盖率
- 按模块分解
- 列出未类型化的函数和参数
- 优先级排序（公共 API 优先）

### 11.2 异步函数地图

```bash
pymap analyze async-map
```

Python 异步编程的常见陷阱：
- 在 async 函数中调用阻塞函数
- 忘记 await
- 同步/异步边界不清

### 11.3 装饰器使用分析

```bash
pymap analyze decorator-usage
```

装饰器是 Python 元编程的核心：
- 统计装饰器使用频率
- 识别自定义装饰器
- 追踪装饰器链

### 11.4 动态特性检测

```bash
pymap query symbols --dynamic
```

标记使用了动态特性的符号：
- `__getattr__`, `__setattr__`, `__delattr__`
- `eval()`, `exec()`
- `globals()`, `locals()`
- `importlib.import_module()`
- `type()` 动态类创建

### 11.5 遗留代码检测

```bash
pymap query symbols --legacy
```

识别遗留代码模式：
- 无类型注解的公共 API
- 使用 `# type: ignore` 的位置
- 全局变量使用
- 可变默认参数

---

## 12. 输出格式

所有 CLI 输出为 JSON，与 crabmap 格式一致。

### 12.1 通用结构

| 命令 | kind | 关键字段 |
|------|------|---------|
| `query stats` | `"stats"` | `nodes`, `edges`, `by_kind`, `by_edge` |
| `query summary` | `"summary"` | `hot_symbols[]`, `project`, `stats` |
| `query symbols` | `"symbols"` | `items[]`, `count`, `applied_filters[]` |
| `query symbol NAME` | `"inspect"` | `node{}`, `incoming[]`, `outgoing[]`, `source{}` |
| `query callees/callers` | `"trace"` | `items[]` |
| `query impact` | `"impact"` | `root`, `dependencies[]`, `dependents[]`, `risk{}` |
| `query risk` | `"risk"` | `symbol`, `risk{}`, `impact_summary`, `test_coverage` |
| `query find` | `"find"` | `items[]` |
| `query path` | `"path"` | `found`, `nodes[]` |
| `query export --format mermaid` | `"mermaid"` | `content` |
| `nav map` | `"map"` | `content`, `budget` |
| `nav health` | `"health"` | `score`, `issues[]` |
| `nav quality` | `"quality"` | `score`, `metrics{}` |
| `analyze deps` | `"deps"` | `items[]`, `recompile_impact[]` |
| `analyze fanout` | `"fanout"` | `items[]` |
| `analyze tests` | `"tests"` | `candidate_tests[]`, `targets[]` |
| `analyze hotspots` | `"git"` | `hotspots[]`, `cochange[]` |
| `analyze diff` | `"diff"` | `added_edges[]`, `removed_edges[]` |
| `analyze refactor-order` | `"refactor_order"` | `order[]` |
| `analyze type-coverage` | `"type_coverage"` | `overall`, `by_module[]`, `untyped_functions[]` |
| `analyze async-map` | `"async_map"` | `async_functions[]`, `blocking_calls[]` |
| `analyze decorator-usage` | `"decorator_usage"` | `decorators[]`, `custom_decorators[]` |

### 12.2 错误处理

与 crabmap 一致：
- 符号未找到：exit 1，stderr 包含建议
- 符号歧义：exit 0，返回 JSON 匹配列表
- 路径未找到：返回 `{"found": false}`

---

## 13. 技术栈

### 13.1 核心依赖

| 功能 | 库 | 说明 |
|------|-----|------|
| CLI | `click` 或 `typer` | 命令行框架 |
| AST 解析 | `ast` (内置) | Python AST |
| CST 解析 | `libcst` | 可选，保留注释 |
| 图算法 | `networkx` | 路径搜索、拓扑排序、SCC |
| 序列化 | `json` + `gzip` | 存储 |
| HTTP | `uvicorn` + `starlette` | Web 服务器 |
| 进度条 | `rich` | 终端 UI |
| Git | `pygit2` 或 `gitpython` | Git 集成 |
| 类型检查 | `pyright` (外部) | 可选类型检查器集成 |

### 13.2 项目结构

```
pymap/
├── pyproject.toml
├── src/
│   └── pymap/
│       ├── __init__.py
│       ├── cli.py                # CLI 入口
│       ├── model.py              # 数据模型
│       ├── analyzer/
│       │   ├── __init__.py
│       │   ├── ast_parser.py     # AST 解析器
│       │   ├── cst_parser.py     # CST 解析器 (可选)
│       │   ├── symbol_resolver.py # 符号解析
│       │   ├── import_resolver.py # 导入解析
│       │   ├── decorator_handler.py # 装饰器处理
│       │   ├── type_handler.py   # 类型注解处理
│       │   └── builder.py        # 图构建器
│       ├── query/
│       │   ├── __init__.py
│       │   ├── commands.py       # 查询命令实现
│       │   ├── find.py           # 符号查找
│       │   ├── traversal.py      # 调用追踪
│       │   ├── impact.py         # 影响分析
│       │   ├── risk.py           # 风险评估
│       │   ├── similar.py        # 结构相似
│       │   ├── filter.py         # 符号过滤
│       │   ├── refactor_order.py # 重构顺序
│       │   └── source.py         # 源码提取
│       ├── nav/
│       │   ├── __init__.py
│       │   ├── map.py            # 架构地图
│       │   ├── guide.py          # 入口点指南
│       │   ├── entries.py        # 入口点检测
│       │   ├── clusters.py       # 功能聚类
│       │   ├── quality.py        # 图质量
│       │   └── health.py         # 架构健康
│       ├── analyze/
│       │   ├── __init__.py
│       │   ├── deps.py           # 依赖分析
│       │   ├── fanout.py         # 扇入/扇出
│       │   ├── tests.py          # 测试影响
│       │   ├── hotspots.py       # Git 热点
│       │   ├── diff.py           # 图 diff
│       │   ├── type_coverage.py  # 类型覆盖率
│       │   ├── async_map.py      # 异步地图
│       │   └── decorator_usage.py # 装饰器分析
│       ├── web/
│       │   ├── __init__.py
│       │   ├── server.py         # HTTP 服务器
│       │   ├── api.py            # API 路由
│       │   └── static/           # 前端文件
│       ├── store.py              # 存储
│       ├── config.py             # 配置
│       ├── gitintel.py           # Git 集成
│       ├── llm.py                # LLM 客户端
│       └── term.py               # 终端输出
├── tests/
│   ├── fixtures/sample/          # 测试用 Python 项目
│   └── test_cli.py               # 集成测试
└── web/                          # 前端源码
    ├── index.html
    ├── styles/
    └── src/
```

---

## 14. 与 crabmap 的完整对齐表

### 14.1 功能对齐

| crabmap 功能 | pymap 功能 | 状态 |
|-------------|-----------|------|
| index | index | ✅ 对齐 |
| query stats | query stats | ✅ 对齐 |
| query summary | query summary | ✅ 对齐 |
| query symbols | query symbols | ✅ 对齐 + 扩展 |
| query symbol | query symbol | ✅ 对齐 |
| query file | query file | ✅ 对齐 |
| query module | query module | ✅ 对齐 |
| query callees | query callees | ✅ 对齐 |
| query callers | query callers | ✅ 对齐 |
| query impact | query impact | ✅ 对齐 |
| query risk | query risk | ✅ 对齐 |
| query search | query search | ✅ 对齐 |
| query find | query find | ✅ 对齐 |
| query similar | query similar | ✅ 对齐 |
| query path | query path | ✅ 对齐 |
| query export | query export | ✅ 对齐 |
| nav map | nav map | ✅ 对齐 |
| nav guide | nav guide | ✅ 对齐 |
| nav entries | nav entries | ✅ 对齐 |
| nav clusters | nav clusters | ✅ 对齐 |
| nav quality | nav quality | ✅ 对齐 |
| nav health | nav health | ✅ 对齐 |
| nav ask | nav ask | ✅ 对齐 |
| nav retrieve | nav retrieve | ✅ 对齐 |
| analyze deps | analyze deps | ✅ 对齐 |
| analyze fanout | analyze fanout | ✅ 对齐 |
| analyze tests | analyze tests | ✅ 对齐 |
| analyze hotspots | analyze hotspots | ✅ 对齐 |
| analyze diff | analyze diff | ✅ 对齐 |
| analyze refactor-order | analyze refactor-order | ✅ 对齐 |
| serve | serve | ✅ 对齐 |
| config | config | ✅ 对齐 |
| (无) | analyze type-coverage | 🆕 Python 特有 |
| (无) | analyze async-map | 🆕 Python 特有 |
| (无) | analyze decorator-usage | 🆕 Python 特有 |

### 14.2 数据模型对齐

| crabmap | pymap | 说明 |
|---------|-------|------|
| CodeGraph | CodeGraph | 顶层图 |
| Project | Project | 项目元信息 |
| Package | Package | 包 |
| Node | Node | 节点 |
| Edge | Edge | 边 |
| NodeKind (21) | NodeKind (30+) | Python 更多类型 |
| EdgeKind (10) | EdgeKind (14+) | Python 更多关系 |
| EdgeSource (4) | EdgeSource (3) | Python 无 MIR |
| EdgeCertainty (4) | EdgeCertainty (4) | 一致 |

---

## 15. 实现优先级

### P0 — 核心（必须）
1. 数据模型 (model.py)
2. AST 索引器 (analyzer/)
3. 符号查找 (query/find.py)
4. 调用追踪 (query/traversal.py)
5. 影响分析 (query/impact.py)
6. CLI 入口 (cli.py)
7. 存储 (store.py)

### P1 — 完整功能
8. 风险评估 (query/risk.py)
9. 导航地图 (nav/map.py)
10. 架构健康 (nav/health.py)
11. 依赖分析 (analyze/deps.py)
12. 测试影响 (analyze/tests.py)
13. 导出 (query/export)

### P2 — 增强功能
14. 类型覆盖率 (analyze/type_coverage.py)
15. 异步地图 (analyze/async_map.py)
16. 装饰器分析 (analyze/decorator_usage.py)
17. Git 集成 (gitintel.py)
18. Web 查看器 (web/)

### P3 — 高级功能
19. LLM 集成 (llm.py)
20. 类型检查器集成 (analyzer/cst_parser.py)
21. 重构顺序 (query/refactor_order.py)
22. 结构相似 (query/similar.py)

---

## 16. 测试策略

### 16.1 测试夹具

`tests/fixtures/sample/` — 最小 Python 项目，包含：
- 一个包（`myapp/`）
- 多个模块（`config.py`, `models.py`, `utils.py`, `server.py`）
- 类继承链
- 装饰器使用
- 异步函数
- 类型注解
- 测试文件（`tests/`）

### 16.2 测试类型

- **单元测试**：每个模块的核心函数
- **集成测试**：通过 `subprocess` 调用 CLI，验证 JSON 输出
- **快照测试**：图输出的稳定性

### 16.3 测试数量目标

- P0 完成后：~30 测试
- P1 完成后：~50 测试
- P2 完成后：~70 测试

---

## 17. 发布

### 17.1 包管理

- 使用 `pyproject.toml` (PEP 621)
- 构建系统：`hatchling` 或 `flit`
- 发布到 PyPI

### 17.2 安装

```bash
pip install pymap
# 或
uv tool install pymap
```

### 17.3 版本

语义化版本 (SemVer)：`0.1.0` → `1.0.0`

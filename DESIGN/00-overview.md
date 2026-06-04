# 总览

`agent-memory` 同时是一个可安装的 Codex skill 和一个本地
CLI/runtime。它的目标是为 coding agent 提供可持久化、可检索、按项目
或全局生效的建议性记忆。

## 包结构

仓库包含：

- 一个名为 `agent-memory` 的 Rust crate。
- 一个名为 `agent-memory` 的 Python package，但只暴露
  `agent_memory.lite_bridge`。
- 用于在目标仓库中启动和运行工具的 Rust CLI。
- `SKILL.md` 中的 skill 元数据和操作说明。
- `references/` 下的参考文档。
- `assets/memory.example.yaml` 中的示例配置。

Rust crate 是主 runtime。它负责命令解析、配置、记录、Qdrant/Milvus 后端选择、
检索、embedding、worker 处理、服务生命周期、迁移、dump 导出和 UI
查看器编排。

Python package 刻意保持很窄。它只保留 legacy Milvus Lite bridge。Python 不拥有
记忆语义、评分、迁移策略、服务生命周期或 CLI 行为。

## 运行状态

运行状态保存在当前 memory root 下，通常是项目内的 `.memory/`：

- `.memory/config.json`：由 `init` 创建的规范化 runtime config。
- `.memory/service.json`：常驻服务状态。
- `.memory/service.lock`：防止重复 foreground service 的锁文件。
- `.memory/service.log`：daemon 的 stdout/stderr。
- `.memory/qdrant/<logical-name>-<uuid>/`：默认本地 Qdrant storage path。
- `.memory/qdrant-server.json`：由 agent-memory 启动的 Qdrant binary 状态。
- `.memory/qdrant-server.log`：Qdrant binary 日志。
- `.memory/milvus/<project-slug>-<uuid>.db`：legacy 本地 Milvus Lite 数据库路径。
- `.memory/milvus-lite-server.json`：临时 Attu viewer server 状态。
- `.memory/milvus-lite-server.log`：临时 Milvus Lite server 日志。
- `.memory/dumps/memory-<timestamp>.json`：未指定输出路径时 `memory dump` 的显式导出。

live records 不存储在 JSONL 或 SQLite 中。配置的 vector backend 是 live record store。

## 主要组件

### CLI 层

`src/cli.rs` 定义所有 clap 命令和参数。`src/commands.rs` 保留入口分发，
具体命令实现按职责拆在 `src/commands/` 下。CLI 默认向 stdout 输出面向人的标题和 Markdown 风格表格；传入
`--agent` 时输出 pretty JSON，供 agent、脚本和工具读取。

### 配置层

`src/config.rs` 保留配置模块入口，`src/config/` 拆分类型定义、配置读写、默认值和测试。配置层有两种结构：

- `UserConfig`：来自 `memory.yaml` 的可编辑 YAML。
- `RuntimeConfig`：位于 `.memory/config.json` 的规范化 JSON。

user config 控制 install scope、memory root、allowed memory types、collection
name、embedding provider、limits、retrieval、worker、service 和 storage
backend。runtime config 固化 CLI 和 service 实际使用的运行值。

### 发现层

`src/discovery.rs` 查找当前生效的 `memory.yaml`。它先在 AGENTS.md 中查找
`agent-memory` 指针，再检查 fallback config paths。setup 期间它也会写入或
替换 AGENTS.md marker block。

### 记录层

`src/models.rs` 定义 `MemoryRecord`、`SearchResult`、`VectorHit` 和
`ServiceState`。`src/records.rs` 负责内容校验、keys 派生、summary 生成、记录
过滤、通过 storage 读写记录，以及标记记录被访问。

### 存储层

`src/storage.rs` 保留存储模块入口，`src/storage/` 按 backend dispatch、Qdrant server、
Qdrant record 操作、remote Milvus 和 record codec 拆分。存储层抽象默认本地 Qdrant、legacy 本地 Milvus Lite 和远程 Milvus。
Qdrant 通过 REST API 读写 points，并在 endpoint 不可达时直接启动配置的
`qdrant` binary。远程 Milvus 只在健康检查时使用 Rust Milvus SDK；database、
collection、entity、query、delete 和 vector search 操作走 Milvus v2 REST
endpoint。

### 检索层

`src/search.rs` 计算精确文本/key 结果、向量结果、合并结果、reliability、
relevance 和最终 score。它也实现了 `search` 当前使用的 associative key
扩展行为。

### Embedding 层

`src/embedding.rs` 支持：

- `fake`：用于测试和简单本地使用的确定性 hash vector。
- `ollama`：`/api/embed`，并 fallback 到 `/api/embeddings`。
- `openai`：使用 `OPENAI_API_KEY` 的 OpenAI-compatible embeddings。

### Worker 层

`src/worker.rs` 处理 pending records，也可以处理 failed records。它会把记录
状态设为 `embedding`，调用 embedding provider，成功后写回 vector 并设为
`embedded`，失败时记录有限长度的错误文本并设为 `failed`。

### Service 层

`src/service.rs` 保留 service 模块入口，`src/service/` 拆分生命周期、foreground loop、
registry 和 removable storage monitor。`service start` 会启动一个 detached foreground
child process。foreground loop 运行 worker thread 和 monitor thread，写入 service
state，清理已经退出的 registered agent PIDs，并且只在 service state 中出现 stop
request 或 state 消失时退出。

### UI 检查层

`src/commands/ui*.rs` 通过 `service ui start/status/stop` 管理当前 backend 的查看入口。
Qdrant 返回 agent-memory gateway 下的官方 `/dashboard` 代理 URL；legacy Milvus Lite
会启动临时 `milvus-lite server` 供 Attu 检查使用，并且会和正常 Lite 写入互斥。

## 常量

`src/lib.rs` 当前硬编码常量：

- `SCHEMA_VERSION = 2`
- `DEFAULT_PROVIDER = "ollama"`
- `DEFAULT_MODEL = "qwen3-embedding:8b"`
- `DEFAULT_DIM = 4096`
- `DEFAULT_ENDPOINT = "http://localhost:11434"`
- `DEFAULT_COLLECTION = "agent_memory"`
- `MAX_CONTENT_BYTES = 4096`
- `PRIMARY_FIELD = "uuid"`
- `VECTOR_FIELD = "dense_vector"`
- AGENTS managed injection 使用的 marker start/end comments。

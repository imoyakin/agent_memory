# Agent Memory 设计

本文档是当前实现设计的索引。它基于本仓库中的代码，而不是未来架构草图。

`agent-memory` 是面向编码 Agent 的仓库级记忆技能和 CLI。Rust 二进制程序负责命令分发、配置、服务生命周期、记录、搜索、嵌入、迁移、本地 Qdrant 二进制进程和 legacy Milvus 访问。Python 仅保留为窄口径的 Milvus Lite 桥接层。

## 设计文件

- [概览](DESIGN/00-overview.md) 描述软件组成、模块边界、运行时状态和包形态。
- [CLI 与工作流](DESIGN/01-cli-and-workflows.md) 列出所有命令和当前命令流程。
- [配置与发现](DESIGN/02-configuration-and-discovery.md) 描述 `memory.yaml`、`.memory/config.json`、发现顺序、AGENTS.md 注入，以及项目/全局作用域行为。
- [存储与 Schema](DESIGN/03-storage-and-schema.md) 描述 Qdrant 默认后端、记录 schema、legacy Milvus 后端、导出和删除。
- [搜索、嵌入、Worker](DESIGN/04-search-embedding-worker.md) 描述精确搜索、向量搜索、关联搜索、评分、嵌入提供方和 worker 状态转换。
- [服务、UI、脚本](DESIGN/05-service-ui-scripts.md) 描述常驻守护进程、服务状态文件、Qdrant 官方 `/dashboard` 代理、legacy Attu/Milvus Lite 查看器、安装脚本、包装脚本和运维约束。

## 当前公开接口

Rust CLI 命令接口为：

- `init`
- `ps`
- `gateway start`
- `gateway status`
- `gateway stop`
- `gateway projects`
- `gateway attach`
- `gateway detach`
- `gateway remotes`
- `memory discover`
- `memory add`
- `memory search`
- `memory list`
- `memory get`
- `memory delete`
- `memory clear`
- `memory audit`
- `memory migrate`
- `memory dump`
- `service start`
- `service status`
- `service stop`
- `service worker`
- `service ui start`
- `service ui status`
- `service ui stop`

隐藏/内部命令接口为：

- `setup-config`
- `gateway start --foreground`
- `service register`
- `service start --foreground`

## 核心保证

- 实时记忆来源是配置的 vector backend，默认是本地 Qdrant。
- 实时记录不使用 JSONL、SQLite 或旁路任务数据库。
- 记录会立即以 `embedding_status: pending` 写入。
- pending 和 failed 记录保留零向量或既有向量，从而保持固定 vector schema 有效。
- 向量搜索会过滤到 `embedding_status == "embedded"`。
- 记忆是建议性上下文。当前用户指令、仓库事实、官方文档和最新工具输出的优先级高于已存储记忆。
- 项目服务是常驻服务，并保持存活直到 `service stop`。
- UI gateway 使用本机 lease 选主，聚合当前机器上的项目和显式 attach 的远端项目，并把
  Qdrant 官方 `/dashboard` 代理到本机 URL；它不实现自动 SSH tunnel 或 Milvus 协议多库代理。
- 本地 Milvus Lite 数据同一时间只能有一个所有者：普通 service/worker 写入和临时 Attu viewer server 不能并发运行。

## 源码地图

- `src/main.rs` 启动 `agent_memory::run()`。
- `src/lib.rs` 声明模块和共享常量。
- `src/cli.rs` 定义 clap 命令形态。
- `src/commands.rs` 保留命令入口，`src/commands/` 按 render、ps、gateway、setup、memory、service、UI helper 拆分命令编排。
- `src/config.rs` 保留配置模块入口，`src/config/` 按类型、读写、默认值和测试拆分。
- `src/discovery.rs` 负责项目/配置发现和 AGENTS.md 指针写入。
- `src/models.rs` 定义持久化和返回的数据模型。
- `src/records.rs` 负责记录校验、过滤、摘要和访问更新。
- `src/storage.rs` 保留存储模块入口，`src/storage/` 拆分 Qdrant server、Qdrant records、backend dispatch、Milvus remote 和 record codec。
- `src/search.rs` 负责精确/向量/关联结果评分。
- `src/embedding.rs` 负责 fake、Ollama 和 OpenAI 嵌入提供方。
- `src/worker.rs` 处理 pending 和 failed 嵌入。
- `src/service.rs` 保留 service 模块入口，`src/service/` 拆分生命周期、foreground loop、registry 和 removable storage monitor。
- `src/paths.rs` 集中管理运行时路径。
- `src/util.rs` 集中管理时间戳、CSV 解析和 UUID 辅助函数。
- `src/agent_memory/lite_bridge.py` 是本地 Milvus Lite 桥接入口，`lite_bridge_codec.py` 负责 entity/record 转换。
- `scripts/setup.sh` 引导目标仓库。
- `scripts/agent-memory.sh` 运行已构建的二进制程序，或回退到 Cargo。

# 配置和发现

当前实现把可编辑的 user configuration 和规范化的 runtime configuration 分开。

## `memory.yaml`

`memory.yaml` 对应 `UserConfig`。

字段：

- `schema_version`：代码生成的配置默认值为 `2`。
- `install_scope`：默认 `project`；可以是 `global`。
- `memory_root`：默认 `.`；global template 使用 `~`，runtime state 写入 `~/.memory`。
- `allowed_memory_types`：默认是面向 project 的类型集合。
- `collection_name`：runtime 初始化时默认使用 logical database name；project 为项目目录最后一级，global 为当前电脑用户名。
- `embedding`：provider、model、dimension、endpoint。
- `limits.max_content_bytes`：默认 `4096`。
- `retrieval`：default limit、advisory flag、associative config。
- `worker`：mode 和 interval seconds。
- `service`：mode 和 PID check interval seconds。
- `storage`：instance UUID、backend、Qdrant config、Lite config、remote config。

默认 project memory types：

- `code`
- `decision`
- `domain`
- `failure`
- `preference`
- `project`
- `research`

Global config template 会设置：

- `memory_root: ~`
- `allowed_memory_types: [environment, preference]`

`memory add` 命令会强制检查 allowed memory types。对于 global install，它还要求
`--scope global`。

## Embedding Config

默认值：

- provider: `ollama`
- model: `qwen3-embedding:8b`
- dim: `4096`
- endpoint: `http://localhost:11434`

CLI `init` 可以覆盖 provider、model、dim 和 endpoint。runtime config 会把这些值
保存为扁平字段。

## Retrieval Config

默认值：

- `default_limit: 1`
- `advisory_memory: true`
- `associative.enabled: false`
- `associative.limit: 3`
- `associative.min_score: 0.35`
- `associative.strategy: keys_then_embedding`

当前代码在 direct results 之后实现了基于 key 的 associative expansion。
`strategy` 字符串会被保存，但目前不会分支到多种策略。

## Storage Config

`storage.instance_uuid` 由 setup/config defaults 生成，用于派生隔离的本地 storage
paths。逻辑 database name 独立于 UUID：project 使用项目目录名，global 使用当前电脑用户名。

Qdrant：

- `backend: qdrant`
- `qdrant.uri: http://127.0.0.1:6333`
- `qdrant.storage_path: .memory/qdrant/<logical-name>-<uuid-without-dashes>`
- `qdrant.binary: qdrant`

Milvus Lite：

- `backend: milvus_lite`
- `milvus_lite.db_path: .memory/milvus/<logical-name>-<uuid-without-dashes>.db`
- `milvus_lite.bridge: python_process`

远程 Milvus：

- `backend: milvus_remote`
- `milvus_remote.uri`
- `milvus_remote.database: <logical-name>`
- `milvus_remote.collection: memories`
- `milvus_remote.token`

`normalize_storage` 会补齐缺失的 UUID、Qdrant URI/path/binary、Lite path、remote
database 和 remote collection；`init` 会进一步把默认 collection/database 名规范化为
logical database name。

## `.memory/config.json`

Runtime config 对应 `RuntimeConfig`。它保存：

- schema version
- root path
- collection name
- embedding provider/model/dim/endpoint
- created timestamp
- install scope
- allowed memory types
- retrieval config
- storage config

`init` 写入这个文件。如果它已经存在且没有使用 `--force`，代码会更新 schema
version、collection name、retrieval config、storage config 和 allowed memory types。
需要新启动 Qdrant 时，backend 初始化由 `init --start-service` 或 `service start`
的 foreground service 进程执行。
在这个 update path 中，已有 embedding provider/model/dim 不会被重写；除非使用
`--force` 创建新的完整 runtime config。

## Discovery Order

`discover(root)` 会解析 base root 并按顺序查找：

1. AGENTS.md marker block，或包含 `agent-memory`/`agent_memory` 且包含
   `memory.yaml`/`memory.yml` path 的行。
2. `<project>/memory.yaml`
3. `<project>/.memory/memory.yaml`
4. `<project>/.agents/agent_memory/memory.yaml`

未显式传入 `--root` 时，discovery 还会从当前目录向上遍历 ancestors。

`discover_root(root)`：

- 如果传入 explicit root，直接使用。
- 否则从当前目录向上查找，返回第一个包含 `.memory/` 的目录。
- 否则返回当前目录。

`runtime_root(root_arg)`：

- 如果传入 explicit root，直接使用。
- 否则使用发现到的 config 并解析 `memory_root`。
- 否则 fallback 到 `discover_root(None)`。

`resolve_memory_root` 使用 `HOME` 展开 `~`。相对 memory root 会拼到 project root
下，并在可能时 canonicalize。

## AGENTS.md 注入

`setup-config --update-agents` 和默认的 `init` 都会写入 managed marker block。
`init --no-update-agents` 可跳过写入。block 会把 config pointer 和 agent 可执行
的 memory workflow 一起放进用户项目的 AGENTS.md：

```markdown
<!-- agent-memory:config:start -->
Agent Memory configuration: `<path>`.

Agent Memory magic word:
- If the user writes `$agent_memory init`, run `agent-memory --agent init --start-service` for the current project, then report the service status.

Agent Memory usage:
- Before starting a non-trivial task, run `agent-memory --agent memory discover` to load the active memory configuration.
- Search memory before asking the user when prior decisions, repository conventions, user preferences, known failures, domain knowledge, or research may matter: `agent-memory --agent memory search "<query>"`.
- Treat memory as advisory. Current user instructions, live repository contents, official documentation, and fresh tool output override stored memory.
- After making a durable, reusable, evidence-backed discovery, consider writing it with `agent-memory --agent memory add --content "<memory>" --type <type> --source-kind <kind> --source-ref <ref> --confidence <0..1> --keys "<search keys>"`.
- Run embedding work with `agent-memory --agent service worker --once`, or keep the resident service available with `agent-memory --agent service start` and stop it with `agent-memory --agent service stop`.
- Write project-specific memories to project scope and global/user-preference memories to global scope when a global memory config is available.
- If only project memory is configured, write otherwise-global relevant memories to the project memory instead of dropping them.
<!-- agent-memory:config:end -->
```

如果 AGENTS.md 已经有 marker block，只替换 marker block。如果文件存在但没有
marker，则追加一个 `## Agent Memory` section。如果文件不存在，则创建新的
`# Agent Guidelines` 文件。

安装脚本默认调用隐藏的 `agents-hook install`，因此 skill 安装时也会把这段
managed description 注入到当前 target project 的 AGENTS.md，并在缺少默认
config 时创建 `.agents/agent_memory/memory.yaml`。卸载脚本调用
`agents-hook remove`，只删除重新生成后逐字匹配的注入文本；用户编辑过的 marker
block 不会被按 marker 范围删除，也不会通过 backup 覆盖或删除整个 AGENTS.md。

discovery parser 会用 regex 从这个 block 中提取 config path；识别以
`memory.yaml` 或 `memory.yml` 结尾的路径。

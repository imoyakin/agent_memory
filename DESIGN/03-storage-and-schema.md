# 存储和 Schema

Qdrant 是默认 live memory storage backend。本地路径直接启动 `qdrant` 二进制，
不使用 Docker。Milvus Lite 和 remote Milvus 只作为 legacy/显式选择的 backend 保留。
所有 backend 使用同一套逻辑 record model。

## 逻辑 Record Model

`MemoryRecord` 字段：

- `uuid`：primary id 和 memory id。
- `content`：作为 advisory memory 注入的 Markdown/text body。
- `keys`：可搜索字符串。
- `summary`：content summary，目前是规范化后的前 360 个字符。
- `embedding_status`：`pending`、`embedding`、`embedded` 或 `failed`。
- `embedding_error`：最近一次有限长度的 embedding error。
- `embedding_attempts`：worker attempts 计数。
- `memory_type`：调用方提供的 type，会按 allowed types 检查。
- `scope`：调用方提供的 scope，默认 `project`。
- `root_path`：runtime root 字符串。
- `tags`：可搜索/可过滤标签。
- `source_kind`：调用方提供的 source class。
- `source_ref`：调用方提供的 source reference。
- `created_at`：RFC3339 timestamp。
- `updated_at`：RFC3339 timestamp。
- `last_accessed_at`：当 search 返回该 record 时设置。
- `access_count`：当 search 返回该 record 时递增。
- `conflict_count`：保存并参与 reliability scoring。
- `confidence`：调用方提供的 `0.0..1.0`。
- `verified_at`：可选。
- `stale_after_days`：可选。
- `embedding_provider`：写入或迁移 record 时从 runtime config 复制。
- `embedding_model`：从 runtime config 复制。
- `embedding_dim`：从 runtime config 复制。
- `schema_version`：当前代码常量。

`record_value` 在 `--agent` JSON 输出中额外添加 `memory_id`，作为 `uuid` 的 alias。

## Qdrant Schema

Qdrant collection：

- collection：runtime `collection_name`；默认是 logical database name，project 为项目目录名，global 为当前电脑用户名。
- point id：`uuid`。
- vector：默认 dense vector，维度由配置指定，distance 为 `Cosine`。
- payload：保留当前逻辑 record 字段名和序列化方式。

Payload fields：

- `uuid`
- `content`
- `keys`：JSON-encoded array
- `summary`
- `embedding_status`
- `embedding_error`
- `embedding_attempts`
- `memory_type`
- `scope`
- `root_path`
- `tags`：JSON-encoded array
- `source_kind`
- `source_ref`
- `created_at`
- `updated_at`
- `last_accessed_at`
- `access_count`
- `conflict_count`
- `confidence`
- `verified_at`
- `stale_after_days`：`-1` 表示 none
- `embedding_provider`
- `embedding_model`
- `embedding_dim`
- `schema_version`
- `dense_vector`
- `reliability`

Local Qdrant runtime：

- `storage.qdrant.uri`：默认 `http://127.0.0.1:6333`。
- `storage.qdrant.storage_path`：默认 `.memory/qdrant/<logical-name>-<uuid>`。
- `storage.qdrant.binary`：默认 `qdrant`。
- `storage.qdrant.static_content_dir`：可选；未设置时优先使用安装目录的
  `bin/qdrant-static/`，用于 Qdrant 官方 `/dashboard` Web UI。
- `.memory/qdrant-server.json`：记录本次由 agent-memory service 启动的 Qdrant PID、
  parent PID、URI 和 storage path。
- `.memory/qdrant-server.log`：Qdrant stdout/stderr。

当 REST endpoint 不可达时，只有 resident service foreground 进程可以启动配置的
Qdrant binary，并通过环境变量设置 host、HTTP port、gRPC port、storage path 和可用的
static content dir。Qdrant 不再 `setsid()` 脱离；它必须保持为 `agent-memory` service
的直接子进程。gRPC 使用 HTTP port 的下一个端口。不会启动 Docker。

## Milvus Lite Schema

Python bridge 创建 Milvus collection，包含：

- primary field：`uuid`，`VARCHAR`，max length 64。
- vector field：`dense_vector`，`FLOAT_VECTOR`，维度由配置指定。
- index：`dense_vector` 上的 `AUTOINDEX`，metric 为 `COSINE`。
- dynamic fields enabled。

Scalar fields：

- `content`：`VARCHAR(8192)`
- `keys`：`VARCHAR(4096)`，JSON-encoded array
- `summary`：`VARCHAR(1024)`
- `embedding_status`：`VARCHAR(32)`
- `embedding_error`：`VARCHAR(1024)`
- `embedding_attempts`：`INT64`
- `memory_type`：`VARCHAR(32)`
- `scope`：`VARCHAR(32)`
- `root_path`：`VARCHAR(2048)`
- `tags`：`VARCHAR(2048)`，JSON-encoded array
- `source_kind`：`VARCHAR(32)`
- `source_ref`：`VARCHAR(2048)`
- `created_at`：`VARCHAR(64)`
- `updated_at`：`VARCHAR(64)`
- `last_accessed_at`：`VARCHAR(64)`
- `access_count`：`INT64`
- `conflict_count`：`INT64`
- `confidence`：`DOUBLE`
- `verified_at`：`VARCHAR(64)`
- `stale_after_days`：`INT64`，`-1` 表示 none
- `embedding_provider`：`VARCHAR(64)`
- `embedding_model`：`VARCHAR(256)`
- `embedding_dim`：`INT64`
- `schema_version`：`INT64`
- `reliability`：`DOUBLE`

## Remote Milvus Schema

Remote setup 使用 Milvus v2 REST 创建 collection：

- database：`storage.milvus_remote.database`，默认 logical database name。
- collection：`storage.milvus_remote.collection`，默认 `memories`。
- primary field：`uuid`
- primary type：`VarChar`
- `autoId: false`
- vector field：`dense_vector`
- dimension：runtime embedding dimension
- metric：`COSINE`
- primary max length param：`64`

Remote upsert 通过 `/v2/vectordb/entities/upsert` 发送所有逻辑字段、reliability
和 vector。

## Backend Operations

`ensure_backend`：

- Qdrant：确保 REST endpoint 可达且属于当前 root 的 agent-memory service；不可达时
  只有 service foreground 进程可以启动本地 Qdrant binary；然后创建 collection。
  如果 endpoint 已被未受管控的 Qdrant 占用，直接报错而不是复用。
- Milvus Lite：调用 bridge `ensure`。
- Remote：可选用 Rust Milvus SDK 做 health check，然后通过 REST 创建 database 和
  collection。当错误消息包含 `exist` 时，已有 database/collection 错误会被接受。

`upsert_record_to_backend`：

- 确保 backend。
- Qdrant：未提供 vector 时先取 existing vector，否则使用 zero vector；然后通过
  REST upsert point。
- Milvus Lite：向 bridge 发送 record、optional vector 和 reliability。
- Remote：未提供 vector 时先取 existing vector，否则使用 zero vector；然后通过
  REST upsert。

`read_records_from_backend`：

- Qdrant：REST scroll points，读取 payload。
- Milvus Lite：bridge `list`。
- Remote：空 filter REST query。

`get_record_from_backend`：

- Qdrant：REST get point by id，读取 payload。
- Milvus Lite：bridge `get`。
- Remote：REST get by id。

`delete_record_from_backend`：

- 先检查是否存在。
- Qdrant：REST delete point by id。
- Milvus Lite：bridge `delete`。
- Remote：使用 filter `uuid == "<id>"` 的 REST delete。

`pending_records_from_backend`：

- Qdrant：scroll all records 后在 Rust 中过滤 `pending`，retry 时也包含 `failed`。
- Milvus Lite：bridge `pending`。
- Remote：查询所有 records，然后在 Rust 中过滤 `pending`，retry 时也包含 `failed`。

`search_vector_backend`：

- Qdrant：REST query points，filter `embedding_status == embedded`。
- Milvus Lite：bridge `search`。
- Remote：REST vector search。

## Milvus Lite Bridge Commands

`agent_memory.lite_bridge` 支持：

- `ensure --db --collection --dim`
- `upsert --db --collection --dim`
- `list --db --collection`
- `get --db --collection --id`
- `delete --db --collection --id`
- `pending --db --collection --limit [--retry-failed|--no-retry-failed]`
- `search --db --collection --limit`

bridge 会为 `upsert` 和 `search` 从 stdin 读取 JSON，输出 compact JSON，并在 stderr
写入 JSON errors，退出码为 1。

bridge 在 list/get/delete/search 前会显式调用 `load_collection`。

## Vector Preservation

record 在没有 vector 的情况下 upsert 时：

- Lite bridge 会在 existing vector 存在时保留它。
- Remote storage 会在 existing vector 存在时取回它。
- 否则两者都使用配置维度的 zero vector。

这允许 records 在 embedding 就绪前先存在于 vector backend 中。

## Record Serialization

对于当前 payload/schema，list fields 存为 JSON strings：

- `keys`
- `tags`

反序列化接受：

- actual arrays
- JSON-encoded arrays
- comma-separated fallback strings

空字符串 option fields 会变成 `None`。负数 `stale_after_days` 会变成 `None`。

## Dump Export

`dump` 是显式导出，不是 live storage。它写入：

- `config`：已加载 runtime config；加载失败时为 null。
- `records`：所有 serialized records。

默认输出路径是 `.memory/dumps/memory-<safe-timestamp>.json`。

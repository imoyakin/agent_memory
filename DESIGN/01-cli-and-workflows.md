# CLI 和工作流

可执行文件名为 `agent-memory`。全局选项：

- `--root <PATH>`：覆盖当前命令的项目/runtime root 发现结果。
- `--agent`：输出结构化 pretty JSON，供 agent、脚本和工具读取。

默认输出是面向人的 terminal view。它由标题、Markdown 风格 key/value 表、
record/search/process 列表表、warning 列表和短状态摘要组成；空 key/value 字段不展示，
长单元格会截断以保持可读性。这个视图不作为稳定机器接口，agent、脚本、测试和
工具集成必须使用 `--agent` JSON。入口分发位于 `src/commands.rs`，具体命令按职责拆在
`src/commands/`。公开命令分为五个顶层入口：

- `setup`：创建 `memory.yaml`，可选运行 init 和启动 service。
- `init`：初始化 runtime state。
- `ps`：列出当前电脑上的 `agent-memory` 进程。
- `memory ...`：配置发现、记录读写、检索、审计、迁移和导出。
- `service ...`：常驻服务、worker 和本地 UI inspection helper。

## `setup-config` 隐藏命令

用途：创建可编辑的 `memory.yaml`，并可选更新 AGENTS.md。

参数：

- `--output <PATH>`：默认 `.agents/agent_memory/memory.yaml`。
- `--install-scope <project|global>`：默认 `project`。
- `--backend <qdrant|milvus-lite|milvus-remote>`：默认 `qdrant`。
- `--remote-uri <URI>`：setup command 选择 remote backend 时要求提供。
- `--update-agents`：写入 managed AGENTS.md block。后续 `init` 默认也会刷新同一个 block。
- `--force`：覆盖已有配置。

流程：

1. 解析 project root。
2. 如果 output path 不是绝对路径，则按 root 相对路径解析。
3. 写入 YAML `UserConfig` template。
4. 如果指定 `--update-agents`，写入或替换 AGENTS.md marker block。
5. 默认输出 root、config path、AGENTS path 和下一步 init 命令的人类可读表格；
   `--agent` 输出同等内容的 JSON。

## `agents-hook` 隐藏命令

用途：给安装/卸载脚本提供 AGENTS.md hook 的单一实现，避免脚本自己按 marker
范围粗暴编辑。

`agents-hook install`：

- 默认 config path 是 `.agents/agent_memory/memory.yaml`。
- 如果 config 不存在，写入默认 `UserConfig` template 并应用逻辑数据库默认值。
- 写入或刷新 managed AGENTS.md block。

`agents-hook remove`：

- 先从 AGENTS.md 中读取当前 config pointer。
- 重新生成该 pointer 对应的 managed text。
- 只删除完全匹配的 generated text；如果 marker block 被用户修改，保持
  `AGENTS.md` 原样。

## `init`

用途：创建或更新 `.memory/config.json`；带 `--start-service` 时启动当前项目
resident service，并由该 service 确保所选 backend 可用。

参数：

- `--config <PATH>`：显式 user config。
- `--provider <NAME>`
- `--model <NAME>`
- `--dim <N>`
- `--endpoint <URL>`
- `--collection <NAME>`
- `--backend <qdrant|milvus-lite|milvus-remote>`
- `--remote-uri <URI>`
- `--remote-token <TOKEN>`
- `--verify-remote`
- `--force`
- `--no-update-agents`：默认会刷新 AGENTS.md managed block；该参数禁用写入。
- `--start-service`：初始化后启动当前项目 resident service，用于 `$agent_memory init`。

流程：

1. 加载显式 config、发现到的 config，或在没有 config 时创建默认 config。
2. 应用 CLI 对 embedding、collection、backend、remote URI 和 token 的覆盖。
3. 规范化 storage 中由 UUID 派生的路径和 database 名。
4. 如果 user config 文件已存在，则持久化更新后的 user config。
5. 如果 `.memory/config.json` 已存在且没有 `--force`，只更新 schema version、
   retrieval、storage 和 allowed memory types。
6. 否则写入完整 runtime config，包括 root path、collection、embedding、
   install scope、allowed types、retrieval 和 storage。
7. 如果没有 `--start-service`，在已有受控 backend 可用时确保 collection；如果需要
   新启动 Qdrant，要求通过 service path 执行。
8. 默认写入或刷新 project root 的 AGENTS.md managed block，插入 config pointer、
   `--agent` 命令用法和 memory operating rules。
9. 如果指定 `--start-service`，启动当前项目 resident service；foreground service
   进程会启动并父进程化 Qdrant，然后确保 collection。

## `ps`

用途：列出当前电脑上正在运行的 `agent-memory` 进程。默认输出类似 Markdown
table 的列表，左侧第一列是每个进程的工作目录；`--agent` 输出结构化 JSON。

流程：

1. 在 Unix/macOS 上枚举 runtime socket dir 中的 `*.sock`。Linux 默认优先
   `$XDG_RUNTIME_DIR/agent-memory/sockets`；macOS 和没有 `XDG_RUNTIME_DIR` 的
   Unix 默认使用 `/tmp/agent-memory-$UID/sockets`。`AGENT_MEMORY_RUNTIME_DIR`
   可以覆盖 runtime dir。
2. Windows named pipe 没有同等 socket 文件枚举语义，因此保留
   `~/.memory/processes.json` 作为候选索引。
3. 对每个候选 IPC endpoint 请求 `status`。
3. IPC 是唯一运行态真值：只有 IPC 可达且返回 active service 时，才展示为
   `running`。
4. IPC 不可达或返回 inactive 时，剪掉 stale socket 或 registry entry。
5. 展示的 PID 使用 IPC 返回的 `service_pid`，候选索引中的 PID 只作为旧值 fallback。
6. 默认展示 role、scope、workdir、root、status、viewer 和 pid。
7. `--agent` 返回固定 JSON shape：`main`、`memories`、`remotes` 和 `pruned`。
   `main` 是本机 gateway 主进程或 null；`memories` 是本机 active service；
   `remotes` 是显式 attach 的远端 gateway。

## `gateway`

用途：在当前电脑上用 lease 选出一个 UI gateway leader，并聚合所有已注册
`agent-memory` project service 和 viewer 状态。

子命令：

- `gateway start`：启动本机 UI gateway lease holder。
- `gateway status`：显示 gateway lease、状态文件、日志路径和可见项目。
- `gateway stop --timeout-seconds <N>`：请求当前 gateway leader 停止。
- `gateway projects`：只列出 gateway 可见的项目和 viewer 状态。
- `gateway attach --name <alias> --url <forwarded-url> --token <token>`：显式接入一个
  SSH 转发后的远端 gateway。
- `gateway detach --name <alias>`：移除远端 gateway。
- `gateway remotes`：列出已 attach 的远端 gateway。

`gateway start` 参数：

- `--lease-seconds <N>`：默认 `15`，最小按 `3` 处理。
- `--heartbeat-seconds <N>`：默认 `5`，最小按 `1` 处理，且不超过 lease。
- `--host <HOST>`：gateway HTTP/proxy 绑定地址，默认 `127.0.0.1`。
- `--port <PORT>`：gateway HTTP/proxy 端口，默认 `19531`。
- `--foreground`：daemonized child 使用的隐藏 internal mode。

流程：

1. 读取 `~/.memory/ui-gateway.json`。
2. 如果现有 gateway PID 存活、未请求停止且 lease 未过期，直接返回当前 leader。
3. 如果 lock/state stale，删除 `~/.memory/ui-gateway.lock`。
4. 非 foreground 模式启动 detached child：
   - `gateway start --foreground`
   - lease 和 heartbeat 参数
5. foreground 模式用 `create_new` 创建 `~/.memory/ui-gateway.lock`。
6. 写入 `~/.memory/ui-gateway.json`，包含 PID、host、port、heartbeat、lease
   到期时间和时间戳。
7. 绑定 `http://<host>:<port>`，提供 JSON API 和 Qdrant dashboard proxy：
   - `GET /`：gateway JSON 状态。
   - `GET /api/projects`：返回 gateway 可见项目。
   - `GET /api/status`：返回 gateway 状态。
   - `/view/<root_hash>/...`：反向代理本机 Qdrant endpoint。
   - `/remote/<alias>/view/<root_hash>/...`：反向代理 attach 的远端 gateway。
8. 按 heartbeat 刷新 `lease_expires_at`。
9. 看到 `stop_requested_at`、state 消失、PID 被替换或进程退出时，标记 stopped 并删除 lock。

Gateway 是本机协调层和 Qdrant 官方 `/dashboard` 的代理层。它不把多个 Milvus Lite
data dir 伪装成一个 Milvus endpoint；本地 Lite 仍然通过 `service ui start` 在同一时间
暴露一个项目给 Attu。

## `memory discover`

用途：显示当前生效的 memory 配置。

流程：

1. 通过 AGENTS.md 和 fallback paths 查找 config。
2. 如果找到，加载 user config 并解析 runtime root。
3. 默认展示 project root、config path、source、runtime root、install scope 和 storage；
   `--agent` 返回同等字段的 JSON。
4. 如果未找到，返回运行 setup 和 init 的指导信息。

## `memory add`

用途：添加一条 durable memory record。

参数：

- `--content <TEXT>`：必填，非空，最大 4096 bytes。
- `--type <TYPE>`：必填，必须被 runtime config 允许。
- `--source-kind <KIND>`：必填。
- `--source-ref <REF>`：必填。
- `--confidence <0.0..1.0>`：必填。
- `--scope <SCOPE>`：默认 `project`。
- `--tags <CSV>`：可选。
- `--keys <CSV>`：可选；为空时自动派生。

流程：

1. 校验 content 和 confidence。
2. 加载 runtime config。
3. 拒绝禁用的 memory type。
4. 当 install scope 为 `global` 时，拒绝非 global scope。
5. 解析 CSV tags 和 keys。
6. 如果未提供 keys，则从 source ref、tags 和 content 派生 keys。
7. 创建带 UUID 且 `embedding_status = pending` 的 `MemoryRecord`。
8. 从 runtime config 填充 embedding provider/model/dim。
9. 生成 content summary。
10. 确保 backend，并在没有新 vector 的情况下 upsert 记录。storage 会保留已有
    vector，或使用 zero vector。

## `memory search`

用途：为查询返回 advisory memory。

参数：

- `<query>`：必填。
- `--limit <N>`：覆盖 `retrieval.default_limit`。
- `--type <TYPE>`：可选 record filter。
- `--scope <SCOPE>`：可选 record filter。
- `--tags <CSV>`：可选 required tag filter。

流程：

1. 加载 runtime config。
2. 按 type、scope 和 tags 读取并过滤 records。
3. 计算 direct exact text/key results。
4. 如果任一 filtered record 已 embedded，则 embed query 并合并 vector results。
5. 将 direct results 截断到请求值或默认 limit，最小为 1。
6. 如果启用 associative retrieval，则使用 direct matched keys 和 keys 作为 seed，
   查找额外 key-associated records。
7. 标记返回的 records 为 accessed。
8. 默认展示 query、warnings 和 results 表格；`--agent` 返回 JSON。

## `memory list`

用途：按可选 filter 列出 records。

参数：

- `--type <TYPE>`
- `--scope <SCOPE>`
- `--tags <CSV>`

流程：从 backend 读取 records，应用 filters。默认展示 records 表格；`--agent`
返回 serialized records。

## `memory get`

用途：按 memory id 获取一条 record。

流程：解析 runtime root，从 backend 获取 record；未找到时返回错误。

## `memory delete`

用途：按 memory id 删除一条 record。

流程：先检查 record 是否存在，再从 backend 删除。默认展示删除结果；`--agent`
返回 boolean `deleted`。

## `memory clear`

用途：删除当前 root 的全部本地 runtime state。

参数：

- `--yes`：必填。

流程：删除 `.memory/`。这会删除当前 root 下的 runtime config、service state、
本地 Lite 数据、日志、viewer state 和 dumps。

## `memory audit`

用途：检查 memory 和 embedding 健康状态。

输出包括：

- root
- storage config
- record 总数
- 按 `embedding_status` 分组的 records
- 按 `memory_type` 分组的 records
- `pending`、`embedding`、`failed` 的 embedding queue counts

## `service worker`

用途：处理 embedding work。

参数：

- `--once`：CLI 接受该参数；当前 dispatch 始终只运行一轮 worker pass。
- `--limit <N>`
- `--retry-failed`

流程见 [检索、Embedding、Worker](04-search-embedding-worker.md)。

## `service start`

用途：启动或运行 resident memory service。

参数：

- `--config <PATH>`
- `--agent-pid <PID>`：可重复。
- `--pid-check-interval <SECONDS>`
- `--worker-interval <SECONDS>`
- `--retry-failed`
- `--foreground`：daemonized child 使用的隐藏 internal mode。

流程：

1. 解析 active user config。
2. 必要时初始化 runtime config 和 backend。
3. 如果 service 已 active，则注册 live agent PIDs 后退出。
4. 如果不是 foreground，启动带 `service start --foreground` 的 detached child，最多等待
   5 秒直到 service state active，然后返回。
5. 如果是 foreground，则运行 service loop 直到停止。

## `service`

子命令：

- `service start`：启动或运行 resident memory service。
- `service status`：默认展示 active state、PID、root、workdir、memory count 和
  IPC endpoint；`--agent` 返回 registered agent PIDs、state path 和 full service
  state。
- `service stop --timeout-seconds <N>`：写入 stop request 并等待 inactive。
- `service worker --once`：运行一轮 embedding worker pass。
- `service register --agent-pid <PID>`：隐藏命令；把 live agent PIDs 合并进
  service state。
- `service ui start/status/stop`：管理本地 UI inspection helper。

## `memory migrate`

用途：把所有 records 迁移到另一个 storage backend。

参数：

- `--to-backend <qdrant|milvus-lite|milvus-remote>`
- `--remote-uri <URI>`
- `--remote-token <TOKEN>`
- `--new-instance`
- `--verify-remote`

流程：

1. 加载当前 runtime config，并从当前 backend 读取所有 records。
2. 构建目标 storage。带 `--new-instance` 时使用新 UUID；否则保留旧 instance UUID
   并规范化派生名称。
3. 确保目标 backend。
4. 写入更新后的 runtime config。
5. 将每条 migrated record 重置为 pending embedding state，并更新 embedding
   provider/model/dim。
6. 把每条 record upsert 到目标 backend。
7. 以 `retry_failed = true` 运行 worker。
8. 把目标 storage 持久化回发现到的 user config。

## `service ui`

子命令：

- `service ui start`：启动当前 backend 的本地 UI；Qdrant 返回 Qdrant 官方 `/dashboard` 代理 URL，Milvus Lite 启动临时 server 给 Attu。
- `service ui status`：报告 viewer server state。
- `service ui stop`：终止 viewer server process group 或 process。

细节见 [Service、UI、Scripts](05-service-ui-scripts.md)。

## `memory dump`

用途：导出 runtime config 和 records。

参数：

- `--output <PATH>`：可选。

流程：

1. 解析 runtime root。
2. 构建包含 runtime config 和全部 records 的 payload；config 加载失败时为 null。
3. 写入指定 output，或 `.memory/dumps/memory-<timestamp>.json`。
4. 默认展示 dump path；`--agent` 返回 JSON。

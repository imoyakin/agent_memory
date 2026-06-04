# Service、UI、Setup

本文档描述 runtime process management、UI viewer 和 Rust setup entrypoint。

## Resident Service

`service start` 为每个 runtime root 启动一个 resident service。

非 foreground 流程：

1. 解析 active user config。
2. 针对 project root/config 运行 `init`。
3. 检查 service status。
4. 如果已 active，注册 live agent PIDs 并退出。
5. 启动当前 executable，参数为：
   - `--root <project-root>`
   - `service start`
   - `--config <config-path>`
   - `--foreground`
   - repeated `--agent-pid`
   - optional intervals 和 retry flag
6. 在 Unix 上用 `setsid` detach child。
7. 将 stdout/stderr 重定向到 `.memory/service.log`。
8. 最多等待 5 秒，直到 `.memory/service.json` 报告 active PID。

Foreground service loop：

1. 用 `create_new` 创建 `.memory/service.lock`。
2. 如果 lock 存在但 service active，注册 agent PIDs 并返回。
3. 如果 lock 存在但 service stale，删除 lock。
4. 写入初始 `ServiceState`，包含 IPC endpoint 和 `memory_count`。
5. 写入全局 registry：`~/.memory/processes.json`。该 registry 只作为 Windows
   named pipe fallback 和兼容索引；Unix/macOS 的 `ps` 主路径是枚举 runtime
   socket dir。
6. 启动 local IPC socket thread。
7. 启动 worker thread。
8. 启动 monitor thread。
9. 睡眠直到 stop flag 被设置。
10. join threads。
11. 清空 service PID 和 agent PIDs，设置 `stopped_at`，从 registry 注销，删除
    lock，返回 stopped JSON。

## Service State

`ServiceState` 字段：

- `service_pid`
- `agent_pids`
- `install_scope`
- `config_path`
- `root`
- `started_at`
- `updated_at`
- `stopped_at`
- `stop_requested_at`
- `last_worker_error`
- `workdir`
- `memory_count`
- `ipc`

`service status` 优先通过 `ipc` 请求 resident service 返回当前状态；无法连接时
fallback 到读取 `service.json`。

## IPC Socket

service 启动时会创建一个 local IPC socket。代码层统一称为 socket：

- Unix/macOS 使用 filesystem socket：`<runtime-dir>/sockets/<root-hash>.sock`。
- Linux runtime dir 默认优先 `$XDG_RUNTIME_DIR/agent-memory`。
- macOS 和没有 `XDG_RUNTIME_DIR` 的 Unix 默认使用 `/tmp/agent-memory-$UID`，避免
  很长的 macOS `$TMPDIR` 路径触碰 Unix socket path 长度限制。
- `AGENT_MEMORY_RUNTIME_DIR` 可以覆盖 runtime dir。
- Windows 上 namespaced local socket 由底层实现映射到 named pipe。
- filesystem socket parent 会创建为当前用户拥有的目录，并设置为 `0700`。

IPC request/response 使用一行一个 JSON object。当前方法：

- `status`
- `stop`
- `worker.run_once`

`ps` 不再扫描系统进程表。Unix/macOS 上它枚举 runtime socket dir 中的 `*.sock`；
Windows 上它读取 `~/.memory/processes.json` 作为 named pipe 候选索引。两种路径都
必须通过 IPC `status` 验证，IPC 是唯一运行态真值。

## UI Gateway Lease

`gateway start` 启动一个本机 UI gateway lease holder。它负责聚合当前机器上的
project service 和 viewer 状态，并为后续 UI 聚合入口提供唯一 leader。

Gateway state：

- `~/.memory/ui-gateway.json`
- `~/.memory/ui-gateway.lock`
- `~/.memory/ui-gateway.log`

Lease 语义：

1. 只有持有 `ui-gateway.lock` 的 foreground gateway 会刷新
   `lease_expires_at`。
2. 其他节点运行 `gateway start` 时，如果现有 PID 存活、未请求停止且 lease 未过期，
   直接复用当前 leader。
3. 如果 PID 已退出或 lease 过期，后续节点删除 stale lock 并接管。
4. `gateway stop` 写入 `stop_requested_at`，leader 在下一次 heartbeat 后停止并
   删除 lock。

Gateway 是本机 agent-memory main process。它管理本机协调状态、聚合本机和显式
attach 的远端 memory services，并把 Qdrant 官方 Web UI 反向代理到本机 URL。当前
legacy Milvus Lite viewer 仍遵守单 data dir owner 规则。

Qdrant viewer/proxy：

- 默认绑定 `http://127.0.0.1:19531`，可通过 `gateway start --host --port` 覆盖。
- `GET /` 返回 gateway JSON 状态，不再返回手写 HTML viewer。
- `GET /api/projects` 返回 gateway 可见项目，包括仅有 active `service ui` viewer
  的项目和已 attach 的远端项目；`?local=true` 只返回本机项目。
- `GET /api/status` 返回 gateway 状态。
- `/view/<root_hash>/...` 反向代理到本机该 root 的 Qdrant endpoint。
- `/remote/<alias>/view/<root_hash>/...` 反向代理到显式 attach 的远端 gateway。
- `/api/memories` 不再提供自研卡片式 viewer；人类检视使用 Qdrant 官方
  `/dashboard`。

SSH 场景采用显式配对：

1. 远端运行 `agent-memory --agent gateway status`，读取 `attach` 对象中的 url/token。
2. 用户自己建立 SSH 转发，例如 `ssh -L 19532:127.0.0.1:19531 user@host`。
3. 本机运行 `agent-memory gateway attach --name <alias> --url <forwarded-url> --token <token>`。
4. 本机 gateway 合并远端 `/api/projects?local=true` 并生成
   `/remote/<alias>/view/<root_hash>/dashboard`。

agent-memory 不自动创建 SSH tunnel，也不要求远端主动连回本机。

## Worker Thread

service worker thread 会重复运行：

```text
cmd_worker(root, limit = None, retry_failed = <serve flag>)
```

它按 `--worker-interval` 或 `worker.interval_seconds` 睡眠，最小值为 0.2 秒。
Worker errors 会存入 `last_worker_error`，并截断到 500 个字符。

## Monitor Thread

monitor thread：

- 每秒读取 service state。
- 当 `stop_requested_at` 出现时停止 service。
- 每轮检查 root、`.memory/` 和 Qdrant storage path 是否可访问，并写入
  `.memory/.service-heartbeat`。
- 连续 3 次不可访问或 heartbeat 写入失败时，停止 service，并尝试终止记录的 Qdrant
  PID。
- 周期性清理已经退出的 registered agent PIDs。
- 更新 `updated_at`。
- 如果 state 消失或无法读取，按同一个连续失败计数处理，而不是一次失败立即退出。

PID check interval 来自 `--pid-check-interval` 或
`service.pid_check_interval_seconds`，最小为 1 秒。

Registered agent PIDs 只用于可观测性。它们不控制 service lifetime。

## Stopping Service

`service stop`：

1. 读取 service state。
2. 如果 state 不存在，返回 stopped false。
3. 如果 PID stale，清空 PID 和 PIDs，设置 stopped time，并返回 stopped。
4. 否则写入 `stop_requested_at`。
5. 轮询直到 service inactive 或 timeout。

## UI Viewer

Qdrant：

1. 解析 runtime root 并加载 runtime config。
2. 确保 resident service 已启动；Qdrant 必须是该 service 的直接子进程。
3. 如果 backend 为 `qdrant`，通过 service-owned Qdrant endpoint 提供 dashboard；
   未受 service 管控的同端口 Qdrant 会被拒绝。
4. 启动或复用 agent-memory gateway，并返回
   `/view/<root_hash>/dashboard` Qdrant 官方 UI 代理 URL。
5. 写入全局 UI viewer registry，方便 gateway 聚合展示。

## Legacy Milvus Lite / Attu Viewer

`service ui start` 会通过临时 `milvus-lite server` 暴露本地 Milvus Lite，让 Attu 可以检查
collection。

参数：

- `--host`：默认 `127.0.0.1`。
- `--port`：默认 `19531`。
- `--max-workers`：默认 `10`，传给 server 时最小为 1。
- `--stop-service`：先停止 active agent-memory service。
- `--timeout-seconds`：默认 `5`。

流程：

1. 解析 runtime root 并加载 runtime config。
2. 要求当前 backend 为 `milvus_lite`。
3. 如果 viewer state active，直接返回。
4. 如果 agent-memory service active 且未提供 `--stop-service`，报错，因为 Lite data
   同一时间只能有一个 owner。
5. 如果请求了 stop service，则停止 active service。
6. 确保 backend。
7. 读取 `~/.memory/ui-viewers.json`，如果其他 root 的 viewer 已占用同一 host/port，
   自动终止该 viewer 并更新它的 state。Attu 不能通过一个 Milvus Lite server 同时查看
   多个 Lite data dir，因此默认 `19531` 始终只保留一个 active viewer。
8. 检查所选 host/port 没有未知进程已经接受 TCP connections。
9. 启动：

```text
uv run --project <skill-root> milvus-lite server \
  --data-dir <lite-db-path> \
  --host <host> \
  --port <port> \
  --max-workers <max-workers>
```

10. 在 Unix 上用 `setsid` detach。
11. 写入 `.memory/milvus-lite-server.json`，包含 `database_name`。该名称由
    project name 和 Lite DB 文件名组成，避免用户在 Attu 中混淆项目。
12. 写入全局 UI viewer registry：`~/.memory/ui-viewers.json`。
13. 等待端口接受 TCP connections。
14. 返回 Attu address、project URL 和可能被自动关闭的 `closed_viewers`。

当 host 为 `0.0.0.0` 时，返回的 connect host 是 `127.0.0.1`。当 host 为 `::` 时，
返回的 connect host 是 `::1`。

`service ui status` 返回 server state 和 viewer metadata。

`service ui stop` 先用 `kill -TERM -<pid>` 终止 process group，fallback 到
`kill -TERM <pid>`，然后等待 PID 退出并把 state 标记为 stopped。

## Rust Setup Command

`agent-memory setup` 用于 bootstrap 目标仓库，替代旧的 shell bootstrap。

Options：

- `--target <PATH>`
- `--config <PATH>`
- `--install-scope project|global`
- `--backend milvus-lite|milvus-remote`
- `--remote-uri <URI>`
- `--remote-token <TOKEN>`
- `--verify-remote`
- `--force-template`
- `--no-update-agents`
- `--init`

流程：

1. 解析 target directory。
2. 运行 `uv sync --project <skill-root>`。
3. 如果 config 已存在且未设置 `--force-template`，复用 existing config。
4. 否则写入 `memory.yaml` template。
5. 默认更新 AGENTS.md；`--no-update-agents` 会禁用 setup 阶段和 `--init` 阶段的
   AGENTS.md 写入。
6. 有 `--init` 或 `--start-service` 时，调用 Rust `init` flow 写入 runtime config。
7. 有 `--start-service` 时，初始化后启动 resident service；backend/Qdrant 启动由
   foreground service 进程执行。

`scripts/install-agent-memory.sh` 在 runtime binaries 安装完成后默认调用
`agent-memory --root <target-root> agents-hook install`。这会在当前 target
project 创建默认 `.agents/agent_memory/memory.yaml`（如果缺失）并注入 managed
AGENTS.md description。`--target-root` 控制接收 hook 的项目；`--no-update-agents`
跳过该写入。

`scripts/install-agent-memory.sh --check-updates` 使用 GitHub Releases latest
页面做每周一次的更新检查。安装脚本在 `bin/install-state.json` 中记录
install mode、repo、resolved release tag、platform 和 last update check。binary
install 发现新 release 时静默替换 `bin/agent-memory`，并从
`agent-memory-skill.tar.gz` 刷新 release 包里的 skill 文件。source install 发现新
release 时不自动编译，只输出给 agent 的提示：先询问用户是否更新
`agent_memory`，因为 Cargo rebuild 可能耗时。更新 release asset 不覆盖 target
project 的 `memory.yaml`、`.memory/` 或 `.agents/agent_memory/`。

`scripts/uninstall-agent-memory.sh` 调用 `agents-hook remove` 删除 hook。remove
只删除与当前生成文本完全一致的注入片段；如果用户改过 marker block，脚本保持
AGENTS.md 不变。脚本不通过 backup 覆盖，也不删除 AGENTS.md 文件。

Rust 代码使用 Cargo manifest directory 查找 Python project 以运行 `uv run`
bridge commands。

## Python Package Entrypoints

`pyproject.toml` 定义：

- package name：`agent-memory`
- requires Python：`>=3.10`
- dependencies：`pymilvus[bulk_writer,milvus-lite]>=2.5,<3`、`pyyaml>=6,<7`
- script：`agent-memory-lite-bridge = agent_memory.lite_bridge:main`

Rust 代码不会调用 console script。它直接通过 `python -m agent_memory.lite_bridge`
调用模块。

## Operational Constraints

- 不要让 normal service/worker writes 和 `service ui start` 同时访问同一个 Lite data
  directory。
- 恢复 normal service writes 前，先停止 UI viewer。
- Remote tokens 可以在 init/migrate 时传入，但不应该提交进仓库。
- `clear --yes` 会删除 active root 下的整个 `.memory/` tree。
- `memory dump` 只创建 export；它不是 live database。

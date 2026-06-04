---
name: agent-memory
description: Use agent-memory as a repository-installed memory skill for coding agents. Bootstrap `<skill-root>/bin/agent-memory` with the bundled install script when missing; discover `memory.yaml` from AGENTS.md, `.memory/`, `.agents/agent_memory/`, or repo root; initialize `.memory/`; run one resident service per project root; search advisory memory before history-sensitive work; add durable memories; and keep global installs limited to machine/user-preference memory.
license: Apache-2.0
compatibility: Binary install requires a supported GitHub Release asset for this platform. Source install requires Rust/Cargo. Local Qdrant storage requires a `qdrant` binary on PATH or `storage.qdrant.binary`; Docker is not used.
metadata:
  version: "0.1.0"
allowed-tools: Bash
---

# Agent Memory

This repository is the skill. It includes the instructions, bundled CLI,
configuration templates, and references needed after installation.

Run commands from the target repository with the Rust `agent-memory` binary.
The preferred installed binary is `<skill-root>/bin/agent-memory`.

## Bootstrap First

Before running memory commands, make sure the installed binary exists and give
the installation a chance to check GitHub Releases for a weekly update:

```bash
<skill-root>/scripts/install-agent-memory.sh --check-updates
```

When prompted on first install, choose binary install to download release assets
into `bin/`, or source install to build this checkout locally. Binary install
downloads the Rust CLI. Source install builds the Rust CLI with Cargo. The
choice is recorded in `bin/install-state.json`.

`--check-updates` checks the latest GitHub Release at most once every seven
days. Binary installs update silently by replacing `bin/agent-memory` and
refreshing the release-packaged skill files. Source installs do not compile
automatically; if an update is available, ask the user whether to update
`agent_memory` because rebuilding may take time, then run the source install
command shown by the script only after approval.

The install script injects the managed Agent Memory description into the target
repository's `AGENTS.md` by default and creates
`.agents/agent_memory/memory.yaml` if needed. Use `--target-root <repo>` when
installing from outside the repository that should receive the hook. Use
`--no-update-agents` only when the project must not be touched.

The installed runtime entrypoints are:

- `<skill-root>/bin/agent-memory`

Agents should use `<skill-root>/bin/agent-memory` when the absolute skill path is
known. If only `agent-memory` is available on PATH, that is acceptable for user
shell examples.

## Discovery First

Before searching or writing memory, discover the active config:

```bash
agent-memory --agent memory discover
```

Discovery order:

1. Read `AGENTS.md` for an `agent-memory` config pointer.
2. Check `memory.yaml` at the repository root.
3. Check `.memory/memory.yaml`.
4. Check `.agents/agent_memory/memory.yaml`.

If discovery finds `memory.yaml`, use that config. If no `memory.yaml` and no
`.memory/` exist, tell the agent/user that the skill code environment and memory
config must be initialized first.

To inspect active agent-memory processes on this computer:

```bash
agent-memory --agent ps
```

The output reports the local gateway main process, local memory services,
attached remote gateways, and pruned stale endpoints. Unix/macOS enumerate
runtime socket files; Windows uses `~/.memory/processes.json` as a named-pipe
candidate index. IPC status is the only runtime authority for local memory
services; endpoints whose IPC does not return an active service are removed
before display. With `--agent`, the JSON shape is `main`, `memories`, `remotes`,
and `pruned`.

To coordinate one local UI gateway leader across agents:

```bash
agent-memory --agent gateway start
agent-memory --agent gateway status
```

The gateway uses `~/.memory/ui-gateway.json` plus a short lease. If the leader
process exits or the lease expires, another `gateway start` can take over. The
gateway aggregates project and viewer status.

The same gateway proxies Qdrant's official Web UI at
`http://127.0.0.1:19531/view/<root_hash>/dashboard` by default. For
Qdrant-backed projects, `service ui start` ensures the resident service is
active and returns that dashboard proxy URL for the service-owned Qdrant
process. Direct Qdrant binaries require the Web UI static bundle; the install
script places it in `bin/qdrant-static/`, and advanced installs may override it
with `storage.qdrant.static_content_dir`.

## Terminal Interaction

The default command output is a terminal-oriented human view, not a stable data
contract. It renders headings, key/value tables, record lists, process lists,
warnings, and short status summaries for users. Long table cells may be
truncated for readability, and empty fields are omitted from key/value tables.

Agents, scripts, tests, and documentation examples that need to consume fields
must pass `--agent` and parse JSON instead of scraping the terminal tables. Use
the terminal view only when presenting status directly to a person.

## Magic Word

If the user writes `$agent_memory init`, run:

```bash
<skill-root>/bin/agent-memory --agent init --start-service
<skill-root>/bin/agent-memory --agent service status
```

This initializes the current project if needed, refreshes the managed AGENTS.md
memory hook, and starts the resident service.

## Initialization Is Explicit

Codex does not run `SKILL.md` as an installer and does not automatically
initialize this skill when the skill is discovered. Treat this file as operating
instructions only. Before using memory in a repository, an agent or user must
explicitly install the binary and run the setup command below once for that
target repository.

Initialize from the target repo:

```bash
agent-memory setup
```

The Rust setup command creates `memory.yaml` and writes an AGENTS.md block with
the config pointer, `--agent` commands, and operating rules. `init` refreshes
the same managed block after runtime initialization so future agents can
discover, search, and write memory without reading this `SKILL.md` first. By
default it writes `.agents/agent_memory/memory.yaml` and uses local Qdrant; use
`--config` to override the config path.

Ask the user to edit `memory.yaml` when provider/model/endpoint are not already
known. Then initialize runtime state and start the supervised service:

```bash
agent-memory init --start-service
```

Default local Ollama config:

- provider: `ollama`
- model: `qwen3-embedding:8b`
- dim: `4096`
- endpoint: `http://localhost:11434`

`init --start-service` creates the target runtime `.memory/config.json`, starts
the resident service, ensures the active Qdrant backend, and refreshes the
managed AGENTS.md memory hook unless `--no-update-agents` is used. Memory
records and embedding state are stored only in the configured vector database.
Local Qdrant is started from the configured binary by the resident
`agent-memory` service and stores points under `storage.qdrant.storage_path`.
For configuration details, read `references/configuration.md`.

Each generated `memory.yaml` contains `storage.instance_uuid`. That UUID is used
to derive local Qdrant storage paths. The logical database name comes from the
project directory name, or from the current computer user name for global
installs. Qdrant uses that logical name as the collection name.

## Service Lifecycle

Setup and plain init do not start a background process. Use
`init --start-service` when memory should be available for the current agent
session.

Start one resident daemon service per project root:

```bash
agent-memory --agent service start
```

The `service start` command starts the service in the background and returns
after the daemon is active. The daemon uses threads for embedding work and PID
monitoring. The OS process is named `agent-memory`; Qdrant remains its direct
child process. The service tracks the agent or ancestor process that started it
and exits when all tracked agents are gone, or when explicitly stopped:

```bash
agent-memory --agent service stop
```

Agents may register additional live PIDs for status and lifetime tracking:

```bash
agent-memory --agent service register --agent-pid "$AGENT_PID"
```

Tracked PIDs are pruned periodically, and the service stops once all tracked
agents are gone. If another service is already active for the same root,
`service start` registers any provided live PID and exits instead of starting a
second process. Service logs are written to `.memory/service.log`.

Project installs are isolated by project root. Multiple agents in different
repositories get independent `.memory/` state and independent services.

For global installs, `memory.yaml` must use `install_scope: global`. Global mode
only stores this computer's environment facts and user preferences. Do not write
project code, business logic, domain, decision, research, or failure memory in
global mode; the CLI rejects those memory types.

## When To Search

Search before answering or implementing when the task may depend on prior repo
decisions, user preferences, known failures, domain knowledge, or research:

```bash
agent-memory --agent memory search "query text"
```

By default this returns only the highest-scoring direct memory. Extra related
memories are returned only when `retrieval.associative.enabled` is true in
`memory.yaml`.

If memory is not initialized, do not assume Codex will initialize it from this
`SKILL.md`. Tell the agent/user to run `agent-memory setup` for the target
project, edit the generated config, and then run
`agent-memory init --start-service`.

## Advisory Priority

Memory is advisory. Current user instructions, live repository contents,
official documentation, and fresh tool output outrank stored memory. If memory
and repo truth conflict, follow repo truth for the task and cite the stale
`memory_id` when recommending update or deletion.

When using search results, preserve:

- `memory_id`
- `summary`
- `source_ref`
- `updated_at`
- `confidence`
- `reliability`

## Write Memory

Only write durable, reusable, evidence-backed memory. Do not store secrets,
large logs, temporary debug output, long source files, or unverified guesses.
Analyze the user's request at task start and again before finishing so useful
project or global preferences are not lost.

```bash
agent-memory --agent memory add \
  --content "Short durable memory, no more than 4096 UTF-8 bytes." \
  --type project \
  --source-kind repo \
  --source-ref "path/or/command/or/user-message" \
  --confidence 0.8 \
  --keys "stable phrase,File.swift,functionName,error signature" \
  --tags "tag-a,tag-b"
```

`memory add` inserts the record into the configured vector database immediately with
`embedding_status: pending`. It does not mean the vector index is ready. Process
embeddings with:

```bash
agent-memory --agent service worker --once
```

## Maintenance

Audit status:

```bash
agent-memory memory audit
```

Start the local storage inspection UI:

```bash
agent-memory service ui start
```

For Qdrant configs, open the returned `ui.address`; it is the Qdrant official
`/dashboard` UI served through the local agent-memory gateway. Check and stop
the UI helper with:

```bash
agent-memory service ui status
agent-memory service ui stop
```

Delete stale or false memory only when evidence supports it or the user asks:

```bash
agent-memory memory delete <memory_id>
```

Clear all project memory only on explicit user instruction:

```bash
agent-memory memory clear --yes
```

Uninstall a project integration with:

```bash
<skill-root>/scripts/uninstall-agent-memory.sh --project-root <target-repo>
```

The uninstall script stops the project service, asks whether to dump memory,
optionally clears `.memory/`, and removes only the exact AGENTS.md text that the
skill generated. If the marker block was edited, uninstall leaves `AGENTS.md`
unchanged instead of deleting by marker range or replacing the file from a
backup. Use `--remove-binaries` only when removing the skill-local `bin/`
entrypoints too.

## References

- Read `references/configuration.md` when generating or editing config.
- Read `references/agent-usage.md` when deciding search/write/delete behavior.

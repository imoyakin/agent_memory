---
name: agent-memory
description: Use agent-memory as a repository-installed memory skill for coding agents. Discover `memory.yaml` from AGENTS.md, `.memory/`, `.agents/agent_memory/`, or repo root; initialize the bundled uv runtime and `.memory/`; run one resident service per project root; search advisory memory before history-sensitive work; add durable memories; and keep global installs limited to machine/user-preference memory.
license: Apache-2.0
compatibility: Requires Rust/Cargo, Python 3.10+, uv, and user configuration for the embedding provider. The bundled setup builds the Rust CLI and installs pymilvus[bulk_writer,milvus-lite] in the uv environment for the Lite bridge.
metadata:
  version: "0.1.0"
allowed-tools: Bash
---

# Agent Memory

This repository is the skill. It includes the instructions, bundled CLI,
configuration templates, and references needed after installation.

Run commands from the target repository with the Rust `agent-memory` binary.

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

The output reads `~/.memory/processes.json`, refreshes entries over local IPC
sockets, and returns each process workdir, mode, status, memory count, PID, IPC
endpoint, and service metadata as JSON. Without `--agent`, the CLI prints
human-readable tables. Global memory uses the user home root and stores runtime
state under `~/.memory`.

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
agent-memory --agent init --start-service
agent-memory --agent service status
```

This initializes the current project if needed, refreshes the managed AGENTS.md
memory hook, and starts the resident service.

## Initialization Is Explicit

Codex does not run `SKILL.md` as an installer and does not automatically
initialize this skill when the skill is discovered. Treat this file as operating
instructions only. Before using memory in a repository, an agent or user must
explicitly run the setup command below once for that target repository.

Initialize from the target repo:

```bash
agent-memory setup
```

The Rust setup command runs `uv sync` for this skill, creates `memory.yaml`, and writes
an AGENTS.md block with the config pointer, `--agent` commands, and operating
rules. `init` refreshes the same managed block after runtime initialization so
future agents can discover, search, and write memory without reading this
`SKILL.md` first. By default it writes `.agents/agent_memory/memory.yaml` and
uses local Milvus Lite; use `--config`, `--backend`, and `--remote-uri` to
override that.

Ask the user to edit `memory.yaml` when provider/model/endpoint are not already
known. Then initialize runtime state:

```bash
agent-memory setup --init
```

Default local Ollama config:

- provider: `ollama`
- model: `qwen3-embedding:8b`
- dim: `4096`
- endpoint: `http://localhost:11434`

`--init` creates the target runtime `.memory/config.json`, active Milvus
backend, and managed AGENTS.md memory hook unless `--no-update-agents` is used.
Memory records and embedding state are stored only in Milvus. Local Milvus Lite
is accessed through the bundled Python bridge; remote Milvus uses the Rust CLI's
remote backend. For configuration details, read
`references/configuration.md`.

Remote initialization:

```bash
agent-memory setup \
  --backend milvus-remote \
  --remote-uri http://localhost:19530 \
  --remote-token root:Milvus \
  --verify-remote \
  --init
```

Each generated `memory.yaml` contains `storage.instance_uuid`. That UUID is used
to derive the Milvus Lite DB path and remote Milvus database name.

## Service Lifecycle

Setup and init do not start a background process unless `init --start-service`
is used. Start the resident service separately when memory should stay
available for the project.

Start one resident daemon service per project root:

```bash
agent-memory --agent service start
```

The `service start` command starts the service in the background and returns
after the daemon is active. The daemon uses threads for embedding work and PID
monitoring. The OS process is named `agent-memory` and detaches from the
invoking shell or agent session. It stays running until explicitly stopped:

```bash
agent-memory --agent service stop
```

Agents may optionally register their live PID for status visibility:

```bash
agent-memory --agent service register --agent-pid "$AGENT_PID"
```

Tracked PIDs are pruned periodically, but they do not control the service
lifetime. If another service is already active for the same root, `service start`
registers any provided live PID and exits instead of starting a second process.
Service logs are written to `.memory/service.log`.

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
project, edit the generated config, and then run `agent-memory setup --init`.

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

`memory add` inserts the record into Milvus immediately with
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

Expose local Milvus Lite to Attu:

```bash
agent-memory service ui start --stop-service
```

Use the returned `attu.address` in Attu, with an empty token. This starts a
separate Milvus Lite server over the same local data directory, so it must not
run at the same time as normal agent-memory service/worker writes. Attu cannot
inspect multiple Milvus Lite data directories through one server; when another
registered viewer already owns the same host/port, `service ui start`
automatically stops it before binding the port. Check and stop it with:

```bash
agent-memory service ui status
agent-memory service ui stop
```

Migrate all records to another backend in one command:

```bash
agent-memory memory migrate \
  --to-backend milvus-remote \
  --remote-uri http://localhost:19530 \
  --new-instance
```

Delete stale or false memory only when evidence supports it or the user asks:

```bash
agent-memory memory delete <memory_id>
```

Clear all project memory only on explicit user instruction:

```bash
agent-memory memory clear --yes
```

## References

- Read `references/configuration.md` when generating or editing config.
- Read `references/agent-usage.md` when deciding search/write/delete behavior.

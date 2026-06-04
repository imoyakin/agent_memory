# Agent Memory

Agent Memory is a local-first memory system for coding agents. It gives an
agent a durable project memory that survives context resets, new sessions, and
different working days without sending the memory database to a hosted service.

Use it when you want an agent to remember project conventions, decisions, known
failures, user preferences, environment facts, and research conclusions, then
retrieve that context before future work.

## What It Does

- Stores durable memories as structured records with content, summary, keys,
  tags, source, confidence, reliability metadata, and embedding state.
- Searches memory through exact matching plus vector search, returning
  advisory results that the agent must compare against current repo truth.
- Runs one resident service per project root to supervise embedding work,
  Qdrant, IPC status, and local process discovery.
- Uses local Qdrant storage by default. Docker is not required.
- Installs as a Codex skill with a stable `bin/agent-memory` entrypoint.
- Injects a managed `AGENTS.md` block so future agents know how to discover,
  search, and write memory for the current repository.
- Provides a local gateway and Qdrant dashboard proxy for visual inspection.
- Supports binary installs from GitHub Releases or local source builds with
  Cargo.

Memory is advisory. Current user instructions, live repository contents,
official documentation, and fresh tool output always outrank stored memory.

## Install

Clone this repository as a skill directory:

```bash
git clone https://github.com/imoyakin/agent_memory ~/.codex/skills/agent-memory
~/.codex/skills/agent-memory/scripts/install-agent-memory.sh
```

The installer offers two modes:

- Binary install downloads `agent-memory-<platform>` from GitHub Releases into
  `bin/`.
- Source install builds this checkout locally with Cargo.

Both modes converge on:

```bash
~/.codex/skills/agent-memory/bin/agent-memory
```

The selected mode, release tag, platform, and weekly update-check time are
stored in `bin/install-state.json`. This file is local runtime metadata and is
not committed.

After installing or updating the skill, restart Codex so the updated `SKILL.md`
is loaded.

## Initialize A Project

Run setup from the repository that should receive memory:

```bash
agent-memory setup
```

Edit the generated `memory.yaml` for your embedding provider, model, dimension,
and endpoint. Then initialize runtime state and start the supervised service:

```bash
agent-memory init --start-service
```

By default, setup writes `.agents/agent_memory/memory.yaml` and records that
path in a managed `AGENTS.md` block. Discovery checks this order:

1. `AGENTS.md` memory config pointer
2. `memory.yaml`
3. `.memory/memory.yaml`
4. `.agents/agent_memory/memory.yaml`

Use `--config` when you want the config at another supported path. Commit
`assets/memory.example.yaml` as a shared example; keep real `memory.yaml` files
local unless your project intentionally wants to share them.

## Daily Agent Workflow

Discover active configuration:

```bash
agent-memory --agent memory discover
```

Search before history-sensitive work:

```bash
agent-memory --agent memory search "release packaging qdrant ownership"
```

Add durable memory after evidence-backed discoveries:

```bash
agent-memory --agent memory add \
  --content "Prefer repo truth over stale memory when answering code questions." \
  --type preference \
  --source-kind user \
  --source-ref "user instruction" \
  --keys "repo truth,stale memory,user preference" \
  --confidence 0.95
```

Embed pending records:

```bash
agent-memory --agent service worker --once
```

Human-readable output is the default. Agents, scripts, tests, and integrations
should pass `--agent` and parse JSON instead of scraping terminal tables.

## Runtime Model

Runtime state is local to the project root unless `--root` or `memory_root`
points elsewhere:

- `.memory/config.json` stores the normalized runtime config.
- `.memory/service.json`, `.memory/service.lock`, and `.memory/service.log`
  track the resident service.
- `.memory/qdrant/<project>-<storage.instance_uuid>/` stores local Qdrant data.
- `.memory/qdrant-server.json` and `.memory/qdrant-server.log` track the
  service-owned Qdrant process.

Records live only in the configured vector database. There is no live JSONL,
SQLite, or ad hoc file database. New records are inserted with
`embedding_status: pending`; the worker updates the same point through
`embedding`, `embedded`, or `failed`.

Qdrant is started only by the resident `agent-memory` service. If the configured
endpoint is already occupied by a Qdrant process that is not parented by the
service for this root, startup fails instead of silently reusing the wrong
database.

## Service And UI

Start or reuse the resident service:

```bash
agent-memory service start
agent-memory service status
```

List local Agent Memory processes:

```bash
agent-memory ps
```

Open the local Qdrant dashboard through the Agent Memory gateway:

```bash
agent-memory service ui start
```

The returned URL looks like:

```text
http://127.0.0.1:19531/view/<root_hash>/dashboard
```

The gateway can also attach explicit SSH-forwarded remote gateways:

```bash
ssh -L 19532:127.0.0.1:19531 user@host
agent-memory gateway attach --name workbox --url http://127.0.0.1:19532 --token <token>
agent-memory gateway remotes
```

Agent Memory does not create SSH tunnels and does not initiate callbacks from
remote hosts.

## Updates

Run a weekly release check from the skill root:

```bash
scripts/install-agent-memory.sh --check-updates
```

Binary installs update silently by replacing `bin/agent-memory` and refreshing
release-packaged skill files. Source installs do not rebuild silently; the
script tells the agent to ask the user whether to update because Cargo rebuilds
may take time.

Update checks and release package refreshes do not overwrite the target
repository's `memory.yaml`, `.memory/`, or `.agents/agent_memory/` config.

## Releases

GitHub Releases are generated by CI from version tags:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The release workflow builds and uploads:

- `agent-memory-darwin-arm64`
- `agent-memory-darwin-x64`
- `agent-memory-linux-x64`
- `agent-memory-windows-x64.exe`
- matching `.sha256` files
- `agent-memory-skill.tar.gz`
- `agent-memory-skill.tar.gz.sha256`

The release skill package contains the skill text, installer scripts, Rust
source, assets, references, and design docs. It does not contain local
`memory.yaml`, `.memory/`, `.agents/`, or `bin/` runtime state.

Release binaries use the Cargo release profile in this repository, which favors
small package size with `opt-level = "z"`, LTO, one codegen unit, stripped
symbols, and `panic = "abort"`.

## Configuration

Default local Ollama embedding config:

```yaml
embedding:
  provider: ollama
  model: qwen3-embedding:8b
  dim: 4096
  endpoint: http://localhost:11434
```

OpenAI-compatible embeddings:

```yaml
embedding:
  provider: openai
  model: text-embedding-3-small
  dim: 1536
  endpoint: https://api.openai.com/v1/embeddings
```

For OpenAI-compatible providers, pass credentials through environment variables
such as `OPENAI_API_KEY`. Do not store API keys in `memory.yaml`.

For the full configuration contract, see `references/configuration.md`.

## Uninstall

Remove a project integration:

```bash
~/.codex/skills/agent-memory/scripts/uninstall-agent-memory.sh --project-root <repo>
```

The uninstall script removes only the exact managed AGENTS block generated by
Agent Memory. If a user edited that marker block, uninstall leaves it untouched.

## Multilingual README

zdoc currently describes itself as a free tool that translates GitHub READMEs
into multiple languages and keeps them up to date:

```text
https://www.zdoc.app/en/imoyakin/agent_memory
```

If zdoc remains free for this repository, use it as the public multilingual
README entrypoint. If it introduces paid requirements or stops serving this
repository, keep the English README as the source of truth and publish
translations from repository-managed Markdown files instead.

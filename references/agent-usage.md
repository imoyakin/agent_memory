# Agent Usage Contract

Use this reference when applying the `agent-memory` skill in a repository.

## Priority

Memory is advisory. Current user instructions, live repository contents,
official documentation, and fresh tool output outrank stored memory.

## Search Before Asking

Before any memory operation, ensure `<skill-root>/bin/agent-memory` exists. If
it is missing, run `<skill-root>/scripts/install-agent-memory.sh` and choose
binary or source install. Binary install downloads release assets into `bin/`;
source install builds the current checkout locally. The install script also
injects the managed Agent Memory hook into the target repository's `AGENTS.md`
by default and creates `.agents/agent_memory/memory.yaml` when needed; pass
`--target-root <repo>` to target a different repository.

Before search, confirm the active config:

```bash
<skill-root>/bin/agent-memory --agent memory discover
```

If discovery reports no config, run the setup path first. The setup path creates
`memory.yaml`, records its path and operating rules in AGENTS.md, and initializes
`.memory/` after the user edits provider settings. The `init` command also
refreshes the managed AGENTS.md block so future agents see the memory hook in
the target project.

Codex does not automatically execute setup from `SKILL.md`. The setup command is
an explicit bootstrap step; after setup/init writes the AGENTS.md pointer and
usage rules, future agents can discover the configured memory path.

When uninstalling, use `<skill-root>/scripts/uninstall-agent-memory.sh
--project-root <repo>`. It removes only the exact text generated for the managed
AGENTS hook. A user-edited marker block is intentionally preserved.

Search memory before asking the user when a task may depend on prior decisions,
repo conventions, user preferences, known failures, domain knowledge, or
research conclusions.

```bash
<skill-root>/bin/agent-memory --agent memory search "query text"
```

The CLI is human-readable by default. Agents and scripts should pass `--agent`
to receive structured JSON.

Do not parse the default terminal view. It is designed for people and may omit
empty fields or truncate long table cells. Use it only when reporting status to
the user; use `--agent` for any follow-up decision, test assertion, or tool
integration.

If a search result matters, carry these fields into the working context:
`memory_id`, `summary`, `source_ref`, `updated_at`, `confidence`, and
`reliability`.

By default, search should return only the highest-scoring direct memory. When
associative retrieval is enabled in `memory.yaml`, results may include extra
related memories. Treat them as weaker context unless the live task evidence
confirms them.

## Write Durable Memory

Write only durable, reusable, evidence-backed memory. Do not store large logs,
temporary debug output, secrets, or unverified guesses. Analyze the user's
request at task start and again before finishing so useful project or global
preferences are not lost.

Each memory should have Markdown `content` and searchable `keys`. Keys should
include stable phrases, paths, symbols, error signatures, aliases, or user
wording that future agents are likely to search for.

```bash
agent-memory --agent memory add \
  --content "Concise durable memory." \
  --type project \
  --source-kind repo \
  --source-ref "path/or/command" \
  --confidence 0.8 \
  --keys "stable phrase,File.swift,functionName,error signature" \
  --tags "tag-a,tag-b"
```

`memory add` means the record is persisted in the configured vector database
with `embedding_status: pending`. It does not mean the vector index is ready.
Run the worker when embedding should be processed:

```bash
agent-memory --agent service worker --once
```

Setup/init only prepares config and runtime files unless `init --start-service`
is used. Start the resident daemon service when memory should remain available
for the project:

```bash
agent-memory --agent service start
```

The command returns after the daemon is active. The service process is named
`agent-memory` and detaches from the invoking shell/session. The service is one
process per root/config and stays running until explicitly stopped:

```bash
agent-memory --agent service stop
```

Agents can optionally register their live PID for status visibility:

```bash
agent-memory --agent service register --agent-pid "$AGENT_PID"
```

Registered PIDs are pruned when they exit, but they do not control the daemon
lifetime.

## Visual Inspection

For local Qdrant, start the storage UI helper and open the returned Qdrant
dashboard proxy URL:

```bash
agent-memory --agent service ui start
```

The URL is served by the local gateway, for example
`http://127.0.0.1:19531/view/<root_hash>/dashboard`, and proxies Qdrant's
official `/dashboard` UI. If a direct Qdrant binary serves the API but
`/dashboard` is unavailable, rerun the install script to provision
`bin/qdrant-static/` or set `storage.qdrant.static_content_dir`.

For SSH use, run `agent-memory --agent gateway status` on the remote, create the
SSH port forward yourself, then attach it locally:

```bash
agent-memory gateway attach --name workbox --url http://127.0.0.1:19532 --token <token>
```

For legacy local Milvus Lite, expose the configured Lite data directory as a
temporary Milvus endpoint for Attu:

```bash
agent-memory --agent service ui start --stop-service
```

Use the returned `attu.address` in Attu with an empty token. The command also
returns the recommended Attu project URL, `https://github.com/zilliztech/attu`.
Attu cannot inspect multiple Milvus Lite data directories through one
`milvus-lite server`; `service ui start` stops any other registered viewer on
the same host/port before binding. While this server is running, do not run the
normal resident service or worker against the same Lite DB. Stop the viewer
server after inspection:

```bash
agent-memory --agent service ui stop
```

## Global Mode

Global installs must use `install_scope: global`. In this mode only
`preference` and `environment` memory types are allowed, and writes should use
`--scope global`. Do not store project code, product decisions, business logic,
domain research, or failure records globally.

When both project and global memory are available, write project-specific facts
to project memory and user/environment preferences to global memory. If only
project memory is configured, write otherwise-global relevant memories to the
project memory instead of dropping them.

## Conflict Handling

If current user wording conflicts with preference memory, follow the current
user wording for this task. If the conflict changes a durable preference, cite
the `memory_id` and ask whether to update or delete the old memory.

If repository truth or official docs prove a memory stale, report the evidence
and update or delete the memory when the user asked for memory maintenance.

## Clear And Delete

Hard delete a specific stale or false memory with:

```bash
agent-memory --agent memory delete <memory_id>
```

Clear all project memory only on explicit user instruction:

```bash
agent-memory --agent memory clear --yes
```

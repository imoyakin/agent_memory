# Agent Memory

Project-local memory skill and tools for coding agents.

This repository is laid out as an installable Codex skill. Clone or install the
repository root as the skill directory; the root contains `SKILL.md`, bundled
CLI code, `references/`, and `assets/`.

The operational CLI is the Rust `agent-memory` binary. Installed skills should
use `bin/agent-memory` as the stable entrypoint. The Python package only exposes
the Milvus Lite bridge used by that Rust CLI; release builds may also include a
platform-specific `bin/agent-memory-lite-bridge` executable so normal Lite
operations do not need to spawn Python through uv.

The storage and retrieval design is maintained in `DESIGN.md`.

Example local install by clone:

```bash
git clone <repo-url> ~/.codex/skills/agent-memory
~/.codex/skills/agent-memory/scripts/install-agent-memory.sh
```

After installing or updating the skill, restart Codex so the new `SKILL.md` is
loaded.

The skill requires `memory.yaml` before memory can work. Discovery checks an
AGENTS.md pointer first, then `memory.yaml`, `.memory/memory.yaml`, and
`.agents/agent_memory/memory.yaml`. The config should normally remain local;
commit `assets/memory.example.yaml` as the shared example instead.

The default root is the current working directory unless a parent `.memory/`
directory already exists or `--root` is provided. Runtime state is stored under
`.memory/` and is intentionally local:

- `.memory/config.json` stores the active embedding profile and schema version.
- `.memory/service.json`, `.memory/service.lock`, and `.memory/service.log` store service lifecycle state.
- `.memory/milvus/<project>-<storage.instance_uuid>.db` is the default Milvus Lite database.

Memory records are stored only in Milvus. There is no live JSONL or SQLite
record/job database. New records are inserted into Milvus immediately with
`embedding_status: pending`; the worker updates the same Milvus row through
`embedding`, `embedded`, or `failed` states and records retry/error metadata in
Milvus fields.

Initialize a project:

```bash
bin/agent-memory setup
# edit memory.yaml
bin/agent-memory setup --init
```

Codex does not automatically run this setup from `SKILL.md`; binary bootstrap
and project initialization are explicit first-use steps for each installation
and target repository.

The install script supports two install modes:

- Binary install downloads `agent-memory-<platform>` and, when available,
  `agent-memory-lite-bridge-<platform>` from GitHub Release assets into `bin/`.
- Source install builds the Rust CLI locally with Cargo and can package the Lite
  bridge locally, falling back to uv when packaging is unavailable.

Both modes converge on `bin/agent-memory`. The script records installation
metadata in `bin/install-state.json`, which is local and ignored by git.

By default setup writes `.agents/agent_memory/memory.yaml` and records that path
plus memory operating rules in `AGENTS.md`. `init` also refreshes the managed
AGENTS.md block so future coding agents know to run `agent-memory --agent memory
discover`, search before history-sensitive work, and write durable memory when
appropriate. Use `--config` to place the config at `.memory/memory.yaml` or
root `memory.yaml` instead. Setup can also choose remote Milvus instead of local
Milvus Lite with flags:

```bash
agent-memory setup \
  --backend milvus-remote \
  --remote-uri http://localhost:19530 \
  --remote-token root:Milvus \
  --verify-remote \
  --init
```

Every generated config contains `storage.instance_uuid`. That UUID derives the
local Lite database path and the remote Milvus database name, avoiding collisions
between projects.

Discover the active config:

```bash
agent-memory memory discover
```

CLI output is human-readable by default. Add `--agent` to any command when an
agent, script, or tool needs structured JSON:

```bash
agent-memory --agent memory discover
```

The human output is the interactive terminal view. It uses concise headings,
Markdown-style key/value tables, record/search/process tables, and status
summaries. Empty key/value fields are hidden and long table cells can be
truncated to keep the view readable. Treat this view as display-only; use
`--agent` for stable field names and complete values.

List running `agent-memory` processes on this computer:

```bash
agent-memory ps
```

The process list reads `~/.memory/processes.json`, refreshes entries over local
IPC sockets, and prints a table with each process workdir first, then root,
mode, status, memory count, and PID. With `--agent`, it returns the same process
data as JSON, including IPC endpoint and service metadata. Global installs use
the user home root and store runtime state under `~/.memory`.

Setup/init prepares local files only unless `init --start-service` is used.

Start the resident daemon service for the project root:

```bash
agent-memory service start
```

`service start` returns after the daemon is active. The OS process is named
`agent-memory` and detaches from the invoking shell/session. The service stays
running until explicitly stopped:

```bash
agent-memory service stop
```

Agents can optionally register their live PID for status visibility:

```bash
agent-memory service register --agent-pid "$AGENT_PID"
```

Only one service runs per project root. In global mode, multiple agent PIDs can
register with the same global service; registered PIDs are pruned when they
exit, but they do not control the daemon lifetime. Global mode only allows
`preference` and `environment` memories.

Add a memory:

```bash
agent-memory memory add \
  --content "Prefer repo truth over stale memory when answering code questions." \
  --type preference \
  --source-kind user \
  --source-ref "user instruction" \
  --keys "repo truth,stale memory,user preference" \
  --confidence 0.95
```

Embed pending records:

```bash
agent-memory service worker --once
```

Search:

```bash
agent-memory memory search "repo truth memory preference"
```

By default, search prints a compact table for humans. With `--agent`, it returns
JSON. Search returns one highest-scoring direct memory by default. When
`retrieval.associative.enabled` is true in `memory.yaml`, it can return extra
related memories marked with `associative_*` match reasons. Search results are
advisory memory and should be checked against current user instructions,
repository contents, and official documents.

Migrate all existing records to a new backend in one command:

```bash
agent-memory memory migrate \
  --to-backend milvus-remote \
  --remote-uri http://localhost:19530 \
  --new-instance
```

Dump records:

```bash
agent-memory memory dump
```

The dump command writes an explicit JSON export of records and runtime config.
It is not a live storage backend.

Visualization:

- For remote Milvus or Milvus Standalone, use a Milvus GUI such as Attu to inspect the `agent_memory` collection, scalar fields, and vectors. If strict open-source licensing is required, pin Attu to the open-source 2.5.x line; newer Attu releases changed licensing.
- For local Milvus Lite, expose the Lite data directory as a local Milvus endpoint, then point Attu at that endpoint:

```bash
agent-memory service ui start --stop-service
```

The command returns an Attu address like `http://127.0.0.1:19530`, recommends
the Attu project at `https://github.com/zilliztech/attu`, and writes server
state/logs to `.memory/milvus-lite-server.json` and
`.memory/milvus-lite-server.log`. Milvus Lite server exposes one data directory
at a time; when another registered viewer already owns the same host/port,
`service ui start` stops that viewer before binding the port. The returned
server metadata includes a project-derived database label so Attu inspection is
less ambiguous. While this server is running, the same Lite DB is locked for
normal agent-memory writes and worker processing. Stop the viewer server before
resuming the resident service:

```bash
agent-memory service ui status
agent-memory service ui stop
```

# Agent Memory

Project-local memory skill and tools for coding agents.

This repository is laid out as an installable Codex skill. Clone or install the
repository root as the skill directory; the root contains `SKILL.md`, bundled
CLI code, `references/`, and `assets/`.

The operational CLI is the Rust `agent-memory` binary. Installed skills should
use `bin/agent-memory` as the stable entrypoint. Local vector storage defaults
to a Qdrant server started from the `qdrant` binary, not Docker. The Python
package only remains for the legacy Milvus Lite bridge.

The storage and retrieval design is maintained in `DESIGN/`.

Example local install by clone:

```bash
git clone <repo-url> ~/.codex/skills/agent-memory
~/.codex/skills/agent-memory/scripts/install-agent-memory.sh
```

The install script also refreshes the current target repository's `AGENTS.md`
hook by default. It creates the default `.agents/agent_memory/memory.yaml` when
needed and injects the managed memory instructions so agents can discover the
config from the project before runtime initialization. Pass `--target-root` to
inject a different project and `--no-update-agents` only when the AGENTS hook
must be skipped.

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
- `.memory/qdrant/<project>-<storage.instance_uuid>/` is the default local Qdrant storage directory.
- `.memory/qdrant-server.json` and `.memory/qdrant-server.log` track the local Qdrant binary process.

Memory records are stored only in the configured vector database. There is no
live JSONL or SQLite record/job database. New records are inserted into Qdrant
immediately with `embedding_status: pending`; the worker updates the same point
through `embedding`, `embedded`, or `failed` states and records retry/error
metadata in payload fields.

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
When Qdrant support is enabled, the script also installs the Qdrant binary and
the official Qdrant Web UI static bundle under `bin/qdrant-static/` so
`/dashboard` works without Docker.

By default setup writes `.agents/agent_memory/memory.yaml` and records that path
plus memory operating rules in `AGENTS.md`. `init` also refreshes the managed
AGENTS.md block so future coding agents know to run `agent-memory --agent memory
discover`, search before history-sensitive work, and write durable memory when
appropriate. Uninstall removes only the exact generated AGENTS text; if that
marker block has been edited by a user, it is left untouched instead of deleting
by marker range. Use `--config` to place the config at `.memory/memory.yaml` or
root `memory.yaml` instead. Setup can still choose remote Milvus instead of
local Qdrant with flags:

```bash
agent-memory setup \
  --backend milvus-remote \
  --remote-uri http://localhost:19530 \
  --remote-token root:Milvus \
  --verify-remote \
  --init
```

Every generated config contains `storage.instance_uuid`. The UUID stays in local
Qdrant and legacy Lite storage paths to avoid directory collisions. The logical
database name is separate: project installs use the last directory name, and
global installs use the current computer user name. Qdrant uses that logical
name as the collection name; remote Milvus uses it as the database and stores
records in a `memories` collection.

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

The process list discovers the local gateway, local IPC endpoints, and attached
remote gateways. Local memory services are validated with live IPC `status`, and
the table shows `role`, `scope`, `workdir`, `root`, `status`, `viewer`, and
`pid`. On Unix/macOS it enumerates runtime socket files:
Linux prefers `$XDG_RUNTIME_DIR/agent-memory/sockets`; macOS and Unix fallback
use `/tmp/agent-memory-$UID/sockets`. `AGENT_MEMORY_RUNTIME_DIR` overrides the
runtime dir. Windows keeps `~/.memory/processes.json` as a named-pipe candidate
index. IPC status is the only runtime authority: endpoints that do not return an
active service are pruned before display. With `--agent`, the JSON shape is
`main`, `memories`, `remotes`, and `pruned`.

Start the local UI gateway lease holder:

```bash
agent-memory gateway start
agent-memory gateway status
```

The gateway writes `~/.memory/ui-gateway.json`, refreshes a short lease while
active, and lists project services plus active viewers from the global
registries. If the gateway process exits or the lease expires, another
`gateway start` can take over. This is local coordination.

`gateway start` is the local main process. It serves JSON status APIs and
proxies Qdrant's official Web UI under
`http://127.0.0.1:19531/view/<root_hash>/dashboard`. For Qdrant projects,
`agent-memory service ui start` starts the Qdrant binary if needed, starts or
reuses the gateway, and returns that proxied dashboard URL. The gateway no
longer serves a custom memory-card UI. Direct Qdrant binaries need Web UI
static files; the install script provisions them at `bin/qdrant-static/`, or
you can set `storage.qdrant.static_content_dir` to an existing static bundle.

Remote SSH use is explicit pairing. On the remote host, run
`agent-memory --agent gateway status` and use the returned `attach` object. Then
create the SSH port forward yourself, for example:

```bash
ssh -L 19532:127.0.0.1:19531 user@host
agent-memory gateway attach --name workbox --url http://127.0.0.1:19532 --token <token>
agent-memory gateway remotes
```

Attached remote projects are exposed through local URLs such as
`/remote/workbox/view/<root_hash>/dashboard`. Agent Memory does not create SSH
tunnels and does not initiate callbacks from the remote host.

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

- For local Qdrant, run `agent-memory service ui start` and open the returned
  Qdrant dashboard proxy URL.
- For a gateway overview, run `agent-memory gateway start`, then inspect
  `agent-memory gateway projects` or `GET /api/projects`.
- For remote Milvus or Milvus Standalone, use a Milvus GUI such as Attu to inspect the `memories` collection, scalar fields, and vectors. If strict open-source licensing is required, pin Attu to the open-source 2.5.x line; newer Attu releases changed licensing.
- For local Milvus Lite, expose the Lite data directory as a local Milvus endpoint, then point Attu at that endpoint:

```bash
agent-memory service ui start --stop-service
```

The command returns an Attu address like `http://127.0.0.1:19531`, recommends
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

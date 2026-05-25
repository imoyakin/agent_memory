# Configuration

Generate an editable config in the target repo:

```bash
agent-memory setup
```

Initialize memory from it:

```bash
agent-memory setup --init
```

Codex skills are passive instruction files. Codex does not automatically run
this setup command just because `SKILL.md` exists or is loaded.

`agent-memory setup` also runs `uv sync` for the installed skill repository.
That makes the skill's Python environment explicit instead of relying only on
lazy `uv run` creation.

The user-editable config is `memory.yaml`. Discovery checks, in order:

- an `agent-memory` pointer in `AGENTS.md`
- `memory.yaml`
- `.memory/memory.yaml`
- `.agents/agent_memory/memory.yaml`

The initialization path reads:

- `install_scope`
- `memory_root`
- `allowed_memory_types`
- `collection_name`
- `embedding.provider`
- `embedding.model`
- `embedding.dim`
- `embedding.endpoint`
- `retrieval.default_limit`
- `retrieval.advisory_memory`
- `retrieval.associative.enabled`
- `retrieval.associative.limit`
- `retrieval.associative.min_score`
- `retrieval.associative.strategy`
- `service.mode`
- `service.pid_check_interval_seconds`
- `worker.mode`
- `worker.interval_seconds`
- `storage.instance_uuid`
- `storage.backend`
- `storage.milvus_lite.db_path`
- `storage.milvus_remote.uri`
- `storage.milvus_remote.database`
- `storage.milvus_remote.collection`
- `storage.milvus_remote.token`

The other keys document local policy for users and future tool surfaces:

- `limits.max_content_bytes`

Project configs default to `install_scope: project` and allow project memory
types. Global configs must use `install_scope: global`; they should use
`memory_root: ~` so runtime state is stored under `~/.memory`, and allow only
`preference` and `environment`.

If the embedding provider or dimension changes after data exists, initialize a
new `.memory` root or rebuild all embeddings. Milvus vector dimensions are a
collection-level schema constraint.

## Storage Backends

Default storage is local Milvus Lite:

```yaml
storage:
  instance_uuid: <generated-uuid>
  backend: milvus_lite
  milvus_lite:
    db_path: .memory/milvus/<project-slug>-<uuid-without-dashes>.db
    bridge: python_process
```

The Python bridge is intentionally narrow: it ensures the Lite collection,
upserts full records, returns record rows, and returns raw vector hits. Record
semantics, scoring, search merging, migration, and resident service control stay
in Rust.

Milvus is the only live memory data source. The record row contains the content,
search keys, metadata, reliability fields, vector, and embedding progress:

- `embedding_status`: `pending`, `embedding`, `embedded`, or `failed`
- `embedding_attempts`: retry count
- `embedding_error`: last bounded failure message

There is no live `.memory/records.jsonl`, `.memory/jobs/embedding.jsonl`, or
SQLite database. `dump` writes an explicit export file only.

## Viewing Local Milvus Lite With Attu

Attu connects to a Milvus endpoint, not directly to the local Lite directory.
For local Lite configs, start a temporary Milvus Lite server:

```bash
agent-memory service ui start --stop-service
```

The command uses `storage.milvus_lite.db_path` as `milvus-lite server
--data-dir`, then returns:

- `attu.address`: the URL to enter in Attu, usually `http://127.0.0.1:19530`
- `attu.project_url`: the recommended Attu project, `https://github.com/zilliztech/attu`
- `server.pid`: the background server process
- `server.log_path`: server logs
- `server.database_name`: project-derived label for the exposed Lite DB

Without `--agent`, `service ui start/status/stop` renders the same state as a
terminal-oriented `UI Inspection Helper` summary. The summary is for humans;
automation should use `--agent` and read the `server` and `attu` JSON objects.

Only one process should own a Milvus Lite data directory at a time. Attu cannot
inspect multiple Milvus Lite data directories through a single `milvus-lite
server`; `service ui start` therefore records active viewers in
`~/.memory/ui-viewers.json` and stops any other registered viewer on the same
host/port before binding, especially the default `127.0.0.1:19530`. Stop the
viewer server before restarting the resident agent-memory service or running
write-heavy CLI operations:

```bash
agent-memory service ui stop
```

Remote Milvus can be selected at setup/init time:

```yaml
storage:
  backend: milvus_remote
  milvus_remote:
    uri: http://localhost:19530
    database: agent_memory_<project-slug>_<uuid-without-dashes>
    collection: agent_memory
    token: null
```

The generated UUID and project slug are used to derive local Lite and remote
database names. Store credentials in environment-specific local config; do not
commit real tokens.

Migrate and re-embed all records into the target backend:

```bash
agent-memory memory migrate \
  --to-backend milvus-remote \
  --remote-uri http://localhost:19530 \
  --new-instance
```

`retrieval.default_limit` defaults to `1`, so normal search returns only the
highest-scoring direct memory. `retrieval.associative` is off by default. When
enabled, the search path may return extra memories related through matched keys
or nearby embeddings, up to `retrieval.associative.limit`. These results must be
clearly marked as associative matches and still remain advisory.

## Provider Examples

Local Ollama:

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

For OpenAI, the CLI reads `OPENAI_API_KEY` from the environment. Do not store
API keys in `memory.yaml`.

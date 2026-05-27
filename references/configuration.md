# Configuration

Generate an editable config in the target repo:

```bash
<skill-root>/bin/agent-memory setup
```

Initialize memory from it:

```bash
<skill-root>/bin/agent-memory setup --init
```

Codex skills are passive instruction files. Codex does not automatically run the
install or setup commands just because `SKILL.md` exists or is loaded.

`scripts/install-agent-memory.sh` installs the runtime entrypoint at
`bin/agent-memory`. Binary mode downloads GitHub Release assets; source mode
builds the Rust CLI locally. The script also runs `uv sync` when uv is
available. By default it also creates the target repository's default
`.agents/agent_memory/memory.yaml` when missing and injects the managed
Agent Memory description into that repository's `AGENTS.md`; use
`--target-root` to choose the repository and `--no-update-agents` to skip this
hook.

The user-editable config is `memory.yaml`. Discovery checks, in order:

- an `agent-memory` pointer in `AGENTS.md`
- `memory.yaml`
- `.memory/memory.yaml`
- `.agents/agent_memory/memory.yaml`

Uninstall removes only the exact generated AGENTS hook text. It does not restore
from a backup, delete `AGENTS.md`, or remove marker-block content that no longer
matches the generated text.

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
- `storage.qdrant.uri`
- `storage.qdrant.storage_path`
- `storage.qdrant.binary`

The other keys document local policy for users and future tool surfaces:

- `limits.max_content_bytes`

Project configs default to `install_scope: project` and allow project memory
types. Global configs must use `install_scope: global`; they should use
`memory_root: ~` so runtime state is stored under `~/.memory`, and allow only
`preference` and `environment`.

If the embedding provider or dimension changes after data exists, initialize a
new `.memory` root or rebuild all embeddings. Vector dimensions are a
collection-level schema constraint.

## Storage

Storage is local Qdrant:

```yaml
storage:
  instance_uuid: <generated-uuid>
  qdrant:
    uri: http://127.0.0.1:6333
    storage_path: .memory/qdrant/<logical-name>-<uuid-without-dashes>
    binary: qdrant
```

The CLI starts Qdrant directly from `storage.qdrant.binary` when the configured
REST endpoint is not already reachable. It sets Qdrant's storage path to
`storage.qdrant.storage_path`, uses the configured REST port for HTTP, and uses
the next port for Qdrant gRPC. When `storage.qdrant.static_content_dir` is set,
or when `bin/qdrant-static/` exists beside the installed binary, the CLI also
passes that directory to Qdrant so the official `/dashboard` Web UI is
available. Docker is not used. Records are stored as Qdrant points whose id is
`uuid`, vector is the active embedding vector, and payload contains the
existing logical record fields.

The configured vector database is the only live memory data source. The record
payload contains the content, search keys, metadata, reliability fields, vector,
and embedding progress:

- `embedding_status`: `pending`, `embedding`, `embedded`, or `failed`
- `embedding_attempts`: retry count
- `embedding_error`: last bounded failure message

There is no live `.memory/records.jsonl`, `.memory/jobs/embedding.jsonl`, or
SQLite database. `dump` writes an explicit export file only.

## Viewing Local Qdrant

For local Qdrant configs, start the storage UI helper and open the returned
Qdrant dashboard proxy URL:

```bash
agent-memory service ui start
```

The command starts the configured Qdrant binary if needed, starts or reuses the
local agent-memory gateway, and returns a URL like
`http://127.0.0.1:19531/view/<root_hash>/dashboard`. That URL proxies Qdrant's
official `/dashboard` UI through the gateway.

Without `--agent`, `service ui start/status/stop` renders the same state as a
terminal-oriented `UI Inspection Helper` summary. The summary is for humans;
automation should use `--agent` and read the `server` and `ui` JSON objects.
Stop the viewer when it is no longer needed:

```bash
agent-memory service ui stop
```

The logical name is the sanitized project directory name for project installs,
or the current computer user name for global installs. The generated UUID stays
in local Qdrant paths to prevent filesystem collisions.

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

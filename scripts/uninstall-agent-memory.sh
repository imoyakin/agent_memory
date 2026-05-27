#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$SKILL_ROOT/bin"
RUST_BIN="${AGENT_MEMORY_BIN:-$BIN_DIR/agent-memory}"
PROJECT_ROOT="$PWD"
REMOVE_BINARIES=0
ASSUME_YES=0

usage() {
  cat <<'EOF'
Usage: scripts/uninstall-agent-memory.sh [options]

Options:
  --project-root <path>     Project root whose generated AGENTS.md hook should be removed.
  --remove-binaries         Also remove installed agent-memory and Qdrant binaries.
  -y, --yes                 Do not prompt; keep memory and remove only the exact AGENTS hook.
  -h, --help                Show this help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --project-root) PROJECT_ROOT="${2:?}"; shift 2 ;;
    --remove-binaries) REMOVE_BINARIES=1; shift ;;
    -y|--yes) ASSUME_YES=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

PROJECT_ROOT="$(cd "$PROJECT_ROOT" && pwd)"

ask() {
  local prompt="$1"
  local default="$2"
  if [[ "$ASSUME_YES" -eq 1 || ! -t 0 ]]; then
    echo "$default"
    return
  fi
  read -r -p "$prompt" answer
  echo "${answer:-$default}"
}

if [[ -x "$RUST_BIN" ]]; then
  "$RUST_BIN" --root "$PROJECT_ROOT" --agent service stop >/dev/null 2>&1 || true
  save_answer="$(ask "Save a memory dump before uninstalling this project? [y/N]: " "n")"
  if [[ "$save_answer" =~ ^[Yy]$ ]]; then
    "$RUST_BIN" --root "$PROJECT_ROOT" memory dump || true
  fi
  delete_answer="$(ask "Delete this project's .memory runtime state? [y/N]: " "n")"
  if [[ "$delete_answer" =~ ^[Yy]$ ]]; then
    "$RUST_BIN" --root "$PROJECT_ROOT" memory clear --yes || true
  fi
fi

AGENTS_FILE="$PROJECT_ROOT/AGENTS.md"
if [[ -f "$AGENTS_FILE" ]]; then
  if [[ -x "$RUST_BIN" ]]; then
    "$RUST_BIN" --root "$PROJECT_ROOT" agents-hook remove >/dev/null
  else
    echo "agent-memory binary not found; leaving AGENTS.md unchanged" >&2
  fi
fi

if [[ "$REMOVE_BINARIES" -eq 1 ]]; then
  rm -f "$BIN_DIR/agent-memory" "$BIN_DIR/qdrant" "$BIN_DIR/install-state.json"
  rm -rf "$BIN_DIR/qdrant-static"
fi

echo "agent-memory project uninstall complete for $PROJECT_ROOT"

#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$SKILL_ROOT/bin"
RUST_BIN="$BIN_DIR/agent-memory"
BRIDGE_BIN="$BIN_DIR/agent-memory-lite-bridge"

MODE="auto"
VERSION="latest"
REPO="${AGENT_MEMORY_GITHUB_REPO:-}"
TARGET_ROOT="$PWD"
INIT_PROJECT=0
BRIDGE_MODE="auto"

usage() {
  cat <<'EOF'
Usage: scripts/install-agent-memory.sh [options]

Options:
  --mode <binary|source|auto>       Install from GitHub Release or build locally.
  --repo <owner/repo>               GitHub repository for binary release assets.
  --version <tag|latest>            Release version to download. Default: latest.
  --target-root <path>              Project root to initialize when --init-project is used.
  --init-project                    Run setup --init --start-service after installation.
  --bridge <auto|binary|uv|none>    Install packaged Python Lite bridge. Default: auto.
  -h, --help                        Show this help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode) MODE="${2:?}"; shift 2 ;;
    --repo) REPO="${2:?}"; shift 2 ;;
    --version) VERSION="${2:?}"; shift 2 ;;
    --target-root) TARGET_ROOT="${2:?}"; shift 2 ;;
    --init-project) INIT_PROJECT=1; shift ;;
    --bridge) BRIDGE_MODE="${2:?}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

case "$MODE" in binary|source|auto) ;; *) echo "--mode must be binary, source, or auto" >&2; exit 2 ;; esac
case "$BRIDGE_MODE" in auto|binary|uv|none) ;; *) echo "--bridge must be auto, binary, uv, or none" >&2; exit 2 ;; esac

mkdir -p "$BIN_DIR"

detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os:$arch" in
    Darwin:arm64) echo "darwin-arm64" ;;
    Darwin:x86_64) echo "darwin-x64" ;;
    Linux:x86_64) echo "linux-x64" ;;
    Linux:aarch64|Linux:arm64) echo "linux-arm64" ;;
    *) echo "unsupported platform: $os $arch" >&2; exit 1 ;;
  esac
}

infer_repo() {
  if [[ -n "$REPO" ]]; then
    echo "$REPO"
    return
  fi
  local url
  url="$(git -C "$SKILL_ROOT" remote get-url origin 2>/dev/null || true)"
  case "$url" in
    git@github.com:*)
      url="${url#git@github.com:}"
      echo "${url%.git}"
      ;;
    https://github.com/*)
      url="${url#https://github.com/}"
      echo "${url%.git}"
      ;;
    *)
      echo ""
      ;;
  esac
}

asset_url() {
  local repo="$1"
  local asset="$2"
  if [[ "$VERSION" == "latest" ]]; then
    echo "https://github.com/$repo/releases/latest/download/$asset"
  else
    echo "https://github.com/$repo/releases/download/$VERSION/$asset"
  fi
}

download_asset() {
  local repo="$1"
  local asset="$2"
  local dest="$3"
  local tmp checksum_tmp
  tmp="$(mktemp)"
  checksum_tmp="$(mktemp)"
  trap 'rm -f "$tmp" "$checksum_tmp"' RETURN
  curl -fL --retry 3 -o "$tmp" "$(asset_url "$repo" "$asset")"
  if curl -fL --retry 3 -o "$checksum_tmp" "$(asset_url "$repo" "$asset.sha256")"; then
    expected="$(awk '{print $1; exit}' "$checksum_tmp")"
    actual="$(shasum -a 256 "$tmp" | awk '{print $1}')"
    if [[ "$expected" != "$actual" ]]; then
      echo "checksum mismatch for $asset" >&2
      exit 1
    fi
  fi
  mv "$tmp" "$dest"
  chmod +x "$dest"
  trap - RETURN
  rm -f "$checksum_tmp"
}

sync_uv() {
  if command -v uv >/dev/null 2>&1; then
    uv sync --project "$SKILL_ROOT"
  elif [[ ! -x "$BRIDGE_BIN" ]]; then
    echo "uv is required when no packaged Lite bridge is installed" >&2
    exit 1
  else
    echo "uv not found; normal memory operations can use the packaged bridge, but UI viewer fallback is unavailable" >&2
  fi
}

install_binary() {
  local platform repo
  platform="$(detect_platform)"
  repo="$(infer_repo)"
  if [[ -z "$repo" ]]; then
    echo "GitHub repo is required for binary install; pass --repo owner/repo or set AGENT_MEMORY_GITHUB_REPO" >&2
    exit 1
  fi
  download_asset "$repo" "agent-memory-$platform" "$RUST_BIN"
  if [[ "$BRIDGE_MODE" == "binary" || "$BRIDGE_MODE" == "auto" ]]; then
    if ! download_asset "$repo" "agent-memory-lite-bridge-$platform" "$BRIDGE_BIN"; then
      if [[ "$BRIDGE_MODE" == "binary" ]]; then
        echo "failed to install packaged Lite bridge" >&2
        exit 1
      fi
      echo "packaged Lite bridge unavailable; falling back to uv bridge" >&2
      rm -f "$BRIDGE_BIN"
    fi
  fi
}

build_source_bridge() {
  if [[ "$BRIDGE_MODE" == "none" || "$BRIDGE_MODE" == "uv" ]]; then
    return
  fi
  if ! command -v uv >/dev/null 2>&1; then
    if [[ "$BRIDGE_MODE" == "binary" ]]; then
      echo "uv is required to build the packaged Lite bridge from source" >&2
      exit 1
    fi
    return
  fi
  mkdir -p "$SKILL_ROOT/target/pyinstaller"
  if ! uv run --project "$SKILL_ROOT" --with pyinstaller pyinstaller \
    --onefile \
    --name agent-memory-lite-bridge \
    --distpath "$BIN_DIR" \
    --workpath "$SKILL_ROOT/target/pyinstaller/build" \
    --specpath "$SKILL_ROOT/target/pyinstaller" \
    "$SKILL_ROOT/src/agent_memory/lite_bridge.py"; then
    if [[ "$BRIDGE_MODE" == "binary" ]]; then
      echo "failed to build packaged Lite bridge" >&2
      exit 1
    fi
    echo "packaged Lite bridge build failed; falling back to uv bridge" >&2
    rm -f "$BRIDGE_BIN"
  fi
}

install_source() {
  command -v cargo >/dev/null 2>&1 || { echo "cargo is required for source install" >&2; exit 1; }
  cargo build --manifest-path "$SKILL_ROOT/Cargo.toml" --release
  cp "$SKILL_ROOT/target/release/agent-memory" "$RUST_BIN"
  chmod +x "$RUST_BIN"
  build_source_bridge
}

choose_mode() {
  if [[ "$MODE" != "auto" ]]; then
    echo "$MODE"
    return
  fi
  if [[ -t 0 ]]; then
    echo "Choose agent-memory install mode:" >&2
    echo "1. Binary install: download release binaries into bin/" >&2
    echo "2. Source install: build this checkout locally" >&2
    read -r -p "Install mode [1/2]: " answer
    case "$answer" in
      2|source) echo "source" ;;
      *) echo "binary" ;;
    esac
  else
    echo "binary"
  fi
}

SELECTED_MODE="$(choose_mode)"
case "$SELECTED_MODE" in
  binary) install_binary ;;
  source) install_source ;;
  *) echo "unknown install mode: $SELECTED_MODE" >&2; exit 1 ;;
esac

sync_uv
"$RUST_BIN" --help >/dev/null
if [[ -x "$BRIDGE_BIN" ]]; then
  "$BRIDGE_BIN" --help >/dev/null
fi

cat > "$BIN_DIR/install-state.json" <<EOF
{
  "install_mode": "$SELECTED_MODE",
  "version": "$VERSION",
  "platform": "$(detect_platform)",
  "rust_binary": "$RUST_BIN",
  "lite_bridge_binary": "$BRIDGE_BIN",
  "lite_bridge_installed": $([[ -x "$BRIDGE_BIN" ]] && echo true || echo false),
  "installed_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
}
EOF

if [[ "$INIT_PROJECT" -eq 1 ]]; then
  "$RUST_BIN" --root "$TARGET_ROOT" setup --init --start-service
  "$RUST_BIN" --root "$TARGET_ROOT" --agent service status
fi

echo "agent-memory installed at $RUST_BIN"

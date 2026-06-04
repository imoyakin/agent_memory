#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$SKILL_ROOT/bin"
HOST_OS="$(uname -s)"
case "$HOST_OS" in
  MINGW*|MSYS*|CYGWIN*) EXE_SUFFIX=".exe" ;;
  *) EXE_SUFFIX="" ;;
esac
RUST_BIN="$BIN_DIR/agent-memory$EXE_SUFFIX"
QDRANT_BIN="$BIN_DIR/qdrant$EXE_SUFFIX"
QDRANT_STATIC_DIR="$BIN_DIR/qdrant-static"
INSTALL_STATE="$BIN_DIR/install-state.json"

MODE="auto"
VERSION="latest"
REPO="${AGENT_MEMORY_GITHUB_REPO:-}"
TARGET_ROOT="$PWD"
INIT_PROJECT=0
UPDATE_AGENTS=1
CHECK_UPDATES=0
QDRANT_MODE="auto"
QDRANT_VERSION="latest"
QDRANT_WEB_UI_VERSION="${AGENT_MEMORY_QDRANT_WEB_UI_VERSION:-latest}"
UPDATE_INTERVAL_SECONDS=$((7 * 24 * 60 * 60))

usage() {
  cat <<'EOF'
Usage: scripts/install-agent-memory.sh [options]

Options:
  --mode <binary|source|auto>       Install from GitHub Release or build locally.
  --repo <owner/repo>               GitHub repository for binary release assets.
  --version <tag|latest>            Release version to download. Default: latest.
  --target-root <path>              Project root to receive AGENTS.md hook and optional init.
  --init-project                    Run setup --init --start-service after installation.
  --no-update-agents                Do not inject or refresh the target AGENTS.md hook.
  --check-updates                   Check GitHub Releases at most weekly and update when safe.
  --qdrant <auto|binary|system|none> Install Qdrant server binary. Default: auto.
  --qdrant-version <tag|latest>      Qdrant release version. Default: latest.
  --qdrant-web-ui-version <tag|latest>
                                      Qdrant Web UI release version. Default: latest.
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
    --no-update-agents) UPDATE_AGENTS=0; shift ;;
    --check-updates) CHECK_UPDATES=1; shift ;;
    --qdrant) QDRANT_MODE="${2:?}"; shift 2 ;;
    --qdrant-version) QDRANT_VERSION="${2:?}"; shift 2 ;;
    --qdrant-web-ui-version) QDRANT_WEB_UI_VERSION="${2:?}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

case "$MODE" in binary|source|auto) ;; *) echo "--mode must be binary, source, or auto" >&2; exit 2 ;; esac
case "$QDRANT_MODE" in auto|binary|system|none) ;; *) echo "--qdrant must be auto, binary, system, or none" >&2; exit 2 ;; esac
TARGET_ROOT="$(cd "$TARGET_ROOT" && pwd)"

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
    MINGW*:x86_64|MSYS*:x86_64|CYGWIN*:x86_64) echo "windows-x64" ;;
    *) echo "unsupported platform: $os $arch" >&2; exit 1 ;;
  esac
}

rust_asset_name() {
  local platform="$1"
  case "$platform" in
    windows-x64) echo "agent-memory-windows-x64.exe" ;;
    *) echo "agent-memory-$platform" ;;
  esac
}

qdrant_asset_name() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os:$arch" in
    Darwin:arm64) echo "qdrant-aarch64-apple-darwin.tar.gz" ;;
    Darwin:x86_64) echo "qdrant-x86_64-apple-darwin.tar.gz" ;;
    Linux:x86_64) echo "qdrant-x86_64-unknown-linux-gnu.tar.gz" ;;
    Linux:aarch64|Linux:arm64) echo "qdrant-aarch64-unknown-linux-musl.tar.gz" ;;
    MINGW*:x86_64|MSYS*:x86_64|CYGWIN*:x86_64) echo "qdrant-x86_64-pc-windows-msvc.zip" ;;
    *) echo "unsupported Qdrant platform: $os $arch" >&2; exit 1 ;;
  esac
}

qdrant_asset_url() {
  local asset="$1"
  if [[ "$QDRANT_VERSION" == "latest" ]]; then
    echo "https://github.com/qdrant/qdrant/releases/latest/download/$asset"
  else
    echo "https://github.com/qdrant/qdrant/releases/download/$QDRANT_VERSION/$asset"
  fi
}

qdrant_web_ui_asset_url() {
  if [[ "$QDRANT_WEB_UI_VERSION" == "latest" ]]; then
    echo "https://github.com/qdrant/qdrant-web-ui/releases/latest/download/dist-qdrant.zip"
  else
    echo "https://github.com/qdrant/qdrant-web-ui/releases/download/$QDRANT_WEB_UI_VERSION/dist-qdrant.zip"
  fi
}

install_qdrant() {
  case "$QDRANT_MODE" in
    none) return ;;
    system)
      command -v qdrant >/dev/null 2>&1 || { echo "qdrant not found on PATH" >&2; exit 1; }
      return
      ;;
  esac
  local asset tmp_dir archive executable
  asset="$(qdrant_asset_name)"
  tmp_dir="$(mktemp -d)"
  archive="$tmp_dir/$asset"
  if ! curl -fL --retry 3 -o "$archive" "$(qdrant_asset_url "$asset")"; then
    rm -rf "$tmp_dir"
    if [[ "$QDRANT_MODE" == "binary" ]]; then
      echo "failed to download Qdrant binary asset $asset" >&2
      exit 1
    fi
    echo "Qdrant binary unavailable; install qdrant on PATH or set storage.qdrant.binary" >&2
    return
  fi
  if [[ "$asset" == *.zip ]]; then
    if command -v unzip >/dev/null 2>&1; then
      unzip -q "$archive" -d "$tmp_dir"
    elif command -v python3 >/dev/null 2>&1; then
      python3 -m zipfile -e "$archive" "$tmp_dir"
    else
      rm -rf "$tmp_dir"
      echo "unzip or python3 is required to extract Qdrant binary asset" >&2
      exit 1
    fi
    executable="$(find "$tmp_dir" -type f -name 'qdrant.exe' | head -n 1)"
  else
    tar -xzf "$archive" -C "$tmp_dir"
    executable="$tmp_dir/qdrant"
  fi
  if [[ -z "$executable" || ! -x "$executable" ]]; then
    rm -rf "$tmp_dir"
    echo "Qdrant archive did not contain executable qdrant" >&2
    exit 1
  fi
  mv "$executable" "$QDRANT_BIN"
  chmod +x "$QDRANT_BIN"
  rm -rf "$tmp_dir"
}

install_qdrant_web_ui() {
  case "$QDRANT_MODE" in
    none) return ;;
  esac
  local tmp_dir archive extracted
  tmp_dir="$(mktemp -d)"
  archive="$tmp_dir/dist-qdrant.zip"
  if ! curl -fL --retry 3 -o "$archive" "$(qdrant_web_ui_asset_url)"; then
    rm -rf "$tmp_dir"
    if [[ "$QDRANT_MODE" == "binary" ]]; then
      echo "failed to download Qdrant Web UI static asset" >&2
      exit 1
    fi
    echo "Qdrant Web UI static asset unavailable; /dashboard proxy will require storage.qdrant.static_content_dir" >&2
    return
  fi
  if command -v unzip >/dev/null 2>&1; then
    unzip -q "$archive" -d "$tmp_dir"
  elif command -v python3 >/dev/null 2>&1; then
    python3 -m zipfile -e "$archive" "$tmp_dir"
  else
    rm -rf "$tmp_dir"
    echo "unzip or python3 is required to extract Qdrant Web UI static asset" >&2
    exit 1
  fi
  extracted="$tmp_dir/dist"
  if [[ ! -f "$extracted/index.html" ]]; then
    rm -rf "$tmp_dir"
    echo "Qdrant Web UI archive did not contain dist/index.html" >&2
    exit 1
  fi
  rm -rf "$QDRANT_STATIC_DIR"
  mkdir -p "$QDRANT_STATIC_DIR"
  cp -R "$extracted"/. "$QDRANT_STATIC_DIR"/
  rm -rf "$tmp_dir"
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

install_binary() {
  local platform repo
  platform="$(detect_platform)"
  repo="$(infer_repo)"
  if [[ -z "$repo" ]]; then
    echo "GitHub repo is required for binary install; pass --repo owner/repo or set AGENT_MEMORY_GITHUB_REPO" >&2
    exit 1
  fi
  download_asset "$repo" "$(rust_asset_name "$platform")" "$RUST_BIN"
}

install_source() {
  command -v cargo >/dev/null 2>&1 || { echo "cargo is required for source install" >&2; exit 1; }
  cargo build --manifest-path "$SKILL_ROOT/Cargo.toml" --release
  cp "$SKILL_ROOT/target/release/agent-memory$EXE_SUFFIX" "$RUST_BIN"
  chmod +x "$RUST_BIN"
}

choose_mode() {
  if [[ "$MODE" != "auto" ]]; then
    echo "$MODE"
    return
  fi
  if [[ -t 0 ]]; then
    echo "Choose how to install agent-memory:" >&2
    echo "1. Binary install: download prebuilt binaries from GitHub Releases into bin/" >&2
    echo "2. Source install: build this checkout locally with Cargo" >&2
    read -r -p "Install mode [1/2]: " answer
    case "$answer" in
      2|source) echo "source" ;;
      *) echo "binary" ;;
    esac
  else
    echo "binary"
  fi
}

. "$SCRIPT_DIR/update-agent-memory.sh"

if [[ "$CHECK_UPDATES" -eq 1 && -x "$RUST_BIN" && -f "$INSTALL_STATE" ]]; then
  check_updates
  exit 0
fi

SELECTED_MODE="$(choose_mode)"

case "$SELECTED_MODE" in
  binary) install_binary ;;
  source) install_source ;;
  *) echo "unknown install mode: $SELECTED_MODE" >&2; exit 1 ;;
esac

install_qdrant
install_qdrant_web_ui
"$RUST_BIN" --help >/dev/null
if [[ -x "$QDRANT_BIN" ]]; then
  "$QDRANT_BIN" --version >/dev/null
fi

if [[ "$UPDATE_AGENTS" -eq 1 ]]; then
  "$RUST_BIN" --root "$TARGET_ROOT" agents-hook install >/dev/null
fi

INSTALL_REPO="$(infer_repo)"
RESOLVED_VERSION="$(resolve_version "$INSTALL_REPO")"
write_install_state "$SELECTED_MODE" "$INSTALL_REPO" "$RESOLVED_VERSION"

if [[ "$INIT_PROJECT" -eq 1 ]]; then
  setup_args=(--root "$TARGET_ROOT" setup --init --start-service)
  if [[ "$UPDATE_AGENTS" -eq 0 ]]; then
    setup_args+=(--no-update-agents)
  fi
  "$RUST_BIN" "${setup_args[@]}"
  "$RUST_BIN" --root "$TARGET_ROOT" --agent service status
fi

echo "agent-memory installed at $RUST_BIN"

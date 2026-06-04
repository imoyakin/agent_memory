latest_release_tag() {
  local repo="$1" effective
  effective="$(curl -fsIL --retry 3 -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")"
  basename "$effective"
}

state_value() {
  local key="$1"
  [[ -f "$INSTALL_STATE" ]] || return 1
  sed -nE 's/^[[:space:]]*"'"$key"'"[[:space:]]*:[[:space:]]*"([^"]*)".*/\1/p' "$INSTALL_STATE" | head -n 1
}

state_number() {
  local key="$1"
  [[ -f "$INSTALL_STATE" ]] || return 1
  sed -nE 's/^[[:space:]]*"'"$key"'"[[:space:]]*:[[:space:]]*([0-9]+).*/\1/p' "$INSTALL_STATE" | head -n 1
}

write_update_check_state() {
  local checked_at_epoch="$1"
  local checked_at_iso
  [[ -f "$INSTALL_STATE" ]] || return
  checked_at_iso="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  local tmp
  tmp="$(mktemp)"
  awk -v checked="$checked_at_epoch" -v checked_iso="$checked_at_iso" '
    BEGIN { wrote = 0 }
    /^[[:space:]]*"last_update_check_epoch"[[:space:]]*:/ {
      print "  \"last_update_check_epoch\": " checked ",";
      wrote = 1;
      next;
    }
    /^[[:space:]]*"last_update_check"[[:space:]]*:/ {
      print "  \"last_update_check\": \"" checked_iso "\",";
      next;
    }
    /^[[:space:]]*"installed_at"[[:space:]]*:/ && !wrote {
      print "  \"last_update_check_epoch\": " checked ",";
      print "  \"last_update_check\": \"" checked_iso "\",";
      wrote = 1;
    }
    { print }
  ' "$INSTALL_STATE" > "$tmp"
  mv "$tmp" "$INSTALL_STATE"
}

install_skill_package() {
  local repo="$1"
  local tmp_dir archive
  tmp_dir="$(mktemp -d)"
  archive="$tmp_dir/agent-memory-skill.tar.gz"
  if ! curl -fL --retry 3 -o "$archive" "$(asset_url "$repo" "agent-memory-skill.tar.gz")"; then
    rm -rf "$tmp_dir"
    echo "skill package asset unavailable; binary was updated but skill files were left unchanged" >&2
    return
  fi
  tar -xzf "$archive" -C "$SKILL_ROOT"
  rm -rf "$tmp_dir"
}

resolve_version() {
  local repo="$1"
  if [[ "$VERSION" == "latest" && -n "$repo" ]]; then
    latest_release_tag "$repo"
  else
    echo "$VERSION"
  fi
}

check_updates() {
  local repo installed_mode installed_version latest now last_check due
  repo="$(infer_repo)"
  if [[ -z "$repo" ]]; then
    echo "agent-memory update check skipped: GitHub repo is unknown; pass --repo owner/repo or set AGENT_MEMORY_GITHUB_REPO" >&2
    return
  fi
  now="$(date -u +%s)"
  last_check="$(state_number last_update_check_epoch || true)"
  if [[ -n "$last_check" ]]; then
    due=$((last_check + UPDATE_INTERVAL_SECONDS))
    if (( now < due )); then
      echo "agent-memory update check skipped: last check was less than 7 days ago"
      return
    fi
  fi
  latest="$(latest_release_tag "$repo")"
  write_update_check_state "$now"
  installed_mode="$(state_value install_mode || true)"
  installed_version="$(state_value resolved_version || state_value version || true)"
  if [[ -z "$installed_version" || "$installed_version" == "latest" ]]; then
    installed_version="$VERSION"
  fi
  if [[ "$installed_version" == "$latest" && -x "$RUST_BIN" ]]; then
    echo "agent-memory is up to date at $latest"
    return
  fi
  if [[ "$installed_mode" == "source" ]]; then
    cat >&2 <<EOF
agent-memory update available: ${installed_version:-unknown} -> $latest.
This install was built from source. Ask the user whether to update agent_memory; updating may take time because it compiles the Rust binary.
If approved, run:
  $0 --mode source --repo $repo --version $latest --target-root "$TARGET_ROOT"
EOF
    return
  fi
  echo "agent-memory update available: ${installed_version:-unknown} -> $latest; updating binary install"
  VERSION="$latest"
  SELECTED_MODE="binary"
  install_binary
  install_skill_package "$repo"
  write_install_state "$SELECTED_MODE" "$repo" "$latest"
  echo "agent-memory updated to $latest"
}

write_install_state() {
  local selected_mode="$1"
  local repo="$2"
  local resolved_version="$3"
  cat > "$INSTALL_STATE" <<EOF
{
  "install_mode": "$selected_mode",
  "repo": "$repo",
  "version": "$VERSION",
  "resolved_version": "$resolved_version",
  "platform": "$(detect_platform)",
  "rust_binary": "$RUST_BIN",
  "qdrant_binary": "$QDRANT_BIN",
  "qdrant_installed": $([[ -x "$QDRANT_BIN" ]] && echo true || echo false),
  "qdrant_static_content_dir": "$QDRANT_STATIC_DIR",
  "qdrant_web_ui_installed": $([[ -f "$QDRANT_STATIC_DIR/index.html" ]] && echo true || echo false),
  "qdrant_web_ui_version": "$QDRANT_WEB_UI_VERSION",
  "last_update_check_epoch": $(date -u +%s),
  "last_update_check": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "installed_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
}
EOF
}

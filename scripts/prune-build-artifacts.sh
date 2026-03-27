#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CODEX_RS_DIR="$ROOT_DIR/codex-rs"
TARGET_DIR="$CODEX_RS_DIR/target"
MODE="${1:-light}"
STALE_DAYS="${STALE_DAYS:-7}"

show_sizes() {
  if [[ -d "$TARGET_DIR" ]]; then
    du -sh "$TARGET_DIR" 2>/dev/null || true
    du -sh "$TARGET_DIR"/* 2>/dev/null | sort -h || true
  else
    echo "target directory not present: $TARGET_DIR"
  fi
}

remove_if_present() {
  local path="$1"
  if [[ -e "$path" ]]; then
    rm -rf "$path"
    echo "removed: $path"
  fi
}

prune_stale_incremental() {
  local incremental_dir="$TARGET_DIR/debug/incremental"
  if [[ ! -d "$incremental_dir" ]]; then
    return
  fi

  while IFS= read -r path; do
    rm -rf "$path"
    echo "removed stale incremental dir: $path"
  done < <(find "$incremental_dir" -mindepth 1 -maxdepth 1 -type d -mtime +"$STALE_DAYS" -print)
}

medium_prune() {
  remove_if_present "$TARGET_DIR/release"
  remove_if_present "$TARGET_DIR/rust-analyzer"
  remove_if_present "$TARGET_DIR/debug/incremental"
}

deep_prune() {
  medium_prune
  (
    cd "$CODEX_RS_DIR"
    cargo clean
  )
}

case "$MODE" in
  sizes)
    show_sizes
    ;;
  light)
    remove_if_present "$TARGET_DIR/release"
    remove_if_present "$TARGET_DIR/rust-analyzer"
    prune_stale_incremental
    show_sizes
    ;;
  medium)
    medium_prune
    show_sizes
    ;;
  deep)
    deep_prune
    show_sizes
    ;;
  *)
    cat >&2 <<'EOF'
usage: scripts/prune-build-artifacts.sh [light|medium|deep|sizes]

  light   Remove release/rust-analyzer outputs and stale debug incremental dirs.
  medium  Remove release/rust-analyzer outputs and all debug incremental dirs.
  deep    Run medium cleanup, then cargo clean for the repo target dir.
  sizes   Print current target size breakdown.
EOF
    exit 1
    ;;
esac

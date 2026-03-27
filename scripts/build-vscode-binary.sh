#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CODEX_RS_DIR="$ROOT_DIR/codex-rs"
SOURCE_BIN="$CODEX_RS_DIR/target/debug/codex"
DEST_DIR="$ROOT_DIR/bin"
DEST_BIN="$DEST_DIR/codex-vscode"

build_binary() {
  (
    cd "$CODEX_RS_DIR"
    env \
      PKG_CONFIG_PATH=/tmp/codex-libcap/extracted/usr/lib/x86_64-linux-gnu/pkgconfig \
      PKG_CONFIG_SYSROOT_DIR=/tmp/codex-libcap/extracted \
      cargo build -p codex-cli
  )
}

copy_binary() {
  mkdir -p "$DEST_DIR"
  local tmp_bin="$DEST_BIN.tmp"
  cp "$SOURCE_BIN" "$tmp_bin"
  chmod 755 "$tmp_bin"
  if command -v strip >/dev/null 2>&1; then
    strip --strip-debug "$tmp_bin" 2>/dev/null || true
  fi
  mv "$tmp_bin" "$DEST_BIN"
}

if [[ "${1:-}" != "--skip-build" ]]; then
  build_binary
fi

if [[ ! -x "$SOURCE_BIN" ]]; then
  echo "missing source binary: $SOURCE_BIN" >&2
  exit 1
fi

copy_binary

echo "VS Code binary ready: $DEST_BIN"
"$DEST_BIN" --version
stat -c 'mtime=%y size=%s' "$DEST_BIN"

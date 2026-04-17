#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CODEX_RS_DIR="$ROOT_DIR/codex-rs"
RUST_MOUNT="/mnt/rust-build"
POWERSHELL_EXE="/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"
WSL_EXE="/mnt/c/Windows/System32/wsl.exe"
RUST_ATTACH_SCRIPT_DEFAULT="$ROOT_DIR/scripts/reattach-rust-disk.ps1"

CACHE_ROOT=""
CACHE_BASE=""
FAST_TARGET_DIR=""
FULL_TEST_TARGET_DIR=""

rust_mount_healthy() {
  mountpoint -q "$RUST_MOUNT" && ls "$RUST_MOUNT" >/dev/null 2>&1
}

usage() {
  cat <<'EOF'
usage: scripts/cargo-workflow.sh <command> [cargo-args...]

commands:
  ensure-mount          Ensure the dedicated rust cache mount is healthy.
  fast [args...]        Run Cargo in fast-local mode.
                        Defaults to: cargo check -p codex-cli
  full-test [args...]   Run Cargo in full-test mode.
                        Defaults to: cargo test
  session-start         Start-of-work cleanup: prune full-test lane, keep fast.
  root                  Print resolved cache root and mount status.
  sizes                 Show current isolated target cache sizes.
  prune-fast            Remove fast-local target cache only.
  prune-full-test       Remove full-test target cache only.
  prune-all             Remove all isolated codex-rs target caches.

notes:
  - Dedicated cache mount is required at /mnt/rust-build by default.
  - If mount is missing, the helper auto-attaches it by default.
  - Set CODEX_AUTO_ATTACH_RUST_DISK=0 to disable auto-attach.
  - Optional timeout (seconds): CODEX_AUTO_ATTACH_TIMEOUT_SECONDS=30
  - Optional cache override: CODEX_RUST_CACHE_ROOT=/your/path
  - Optional attach script override: CODEX_RUST_ATTACH_SCRIPT=/path/script.ps1
EOF
}

resolve_windows_attach_script() {
  local attach_script="$1"
  if [[ ! -f "$attach_script" ]]; then
    echo "ERROR: rust attach script not found at $attach_script" >&2
    return 1
  fi

  if [[ "$attach_script" == /mnt/* ]]; then
    wslpath -w "$attach_script"
    return
  fi

  local user_profile="${USERPROFILE:-}"
  if [[ -z "$user_profile" ]]; then
    echo "ERROR: USERPROFILE is not set; cannot stage attach script" >&2
    return 1
  fi

  if [[ "$user_profile" == *:\\* ]]; then
    user_profile="$(wslpath -u "$user_profile")"
  fi

  local staging_dir="$user_profile/AppData/Local/Temp/codex-rust-cache"
  mkdir -p "$staging_dir"
  local staged_script="$staging_dir/reattach-rust-disk.ps1"
  cp "$attach_script" "$staged_script"
  wslpath -w "$staged_script"
}

auto_attach_rust_mount_via_wsl() {
  local distro="${CODEX_RUST_WSL_DISTRO:-Ubuntu}"
  local vhd_path="${CODEX_RUST_WSL_VHD_PATH:-C:\\WSL\\rust-build.vhdx}"

  if [[ ! -x "$WSL_EXE" ]]; then
    return 1
  fi

  if mountpoint -q "$RUST_MOUNT" && ! rust_mount_healthy; then
    "$WSL_EXE" -d "$distro" -u root -- umount -lf "$RUST_MOUNT" >/dev/null 2>&1 || true
  fi

  "$WSL_EXE" --mount --vhd "$vhd_path" --bare >/dev/null 2>&1 || true
  "$WSL_EXE" -d "$distro" -u root -- mount -a >/dev/null 2>&1 || true
}

auto_attach_rust_mount() {
  local attach_script="${CODEX_RUST_ATTACH_SCRIPT:-$RUST_ATTACH_SCRIPT_DEFAULT}"
  local timeout_seconds="${CODEX_AUTO_ATTACH_TIMEOUT_SECONDS:-30}"

  if [[ ! -x "$POWERSHELL_EXE" ]]; then
    echo "ERROR: powershell executable not found at $POWERSHELL_EXE" >&2
    return 1
  fi

  echo "rust mount missing: attempting auto-attach" >&2

  auto_attach_rust_mount_via_wsl || true

  if ! rust_mount_healthy; then
    local win_attach_script
    if ! win_attach_script="$(resolve_windows_attach_script "$attach_script")"; then
      return 1
    fi

    echo "rust mount still unavailable: falling back to PowerShell helper $attach_script" >&2
    if ! "$POWERSHELL_EXE" \
      -NoProfile \
      -ExecutionPolicy Bypass \
      -File "$win_attach_script" \
      -NoPause \
      ; then
      echo "ERROR: auto-attach helper invocation failed" >&2
      return 1
    fi
  fi

  local waited=0
  while ((waited < timeout_seconds)); do
    if rust_mount_healthy; then
      return 0
    fi
    sleep 2
    waited=$((waited + 2))
  done
  return 1
}

ensure_rust_mount() {
  if rust_mount_healthy; then
    return
  fi

  if [[ "${CODEX_AUTO_ATTACH_RUST_DISK:-1}" == "1" ]]; then
    auto_attach_rust_mount || true
  fi

  if rust_mount_healthy; then
    return
  fi

  cat >&2 <<'EOF'
ERROR: /mnt/rust-build is not mounted.

Reattach from Windows:
  powershell -ExecutionPolicy Bypass -File "$env:USERPROFILE\Desktop\reattach-rust-disk.ps1"

Auto-attach is enabled by default for cargo-workflow.
To disable it explicitly:
  CODEX_AUTO_ATTACH_RUST_DISK=0 /home/rose/work/codex/fork/scripts/cargo-workflow.sh <command>
EOF
  exit 1
}

detect_cache_root() {
  if [[ -n "${CODEX_RUST_CACHE_ROOT:-}" ]]; then
    printf '%s\n' "$CODEX_RUST_CACHE_ROOT"
    return
  fi

  ensure_rust_mount
  printf '%s\n' "$RUST_MOUNT"
}

resolve_cache_paths() {
  if [[ -n "$CACHE_ROOT" ]]; then
    return
  fi

  CACHE_ROOT="$(detect_cache_root)"
  CACHE_BASE="$CACHE_ROOT/cargo-target/codex-rs"
  FAST_TARGET_DIR="$CACHE_BASE/fast"
  FULL_TEST_TARGET_DIR="$CACHE_BASE/full-test"
}

ensure_dirs() {
  resolve_cache_paths
  mkdir -p "$FAST_TARGET_DIR" "$FULL_TEST_TARGET_DIR"
}

show_root() {
  if [[ -n "${CODEX_RUST_CACHE_ROOT:-}" ]]; then
    echo "cache_root=$CODEX_RUST_CACHE_ROOT"
    echo "cache_root_source=env_override"
  else
    echo "cache_root=$RUST_MOUNT"
    echo "cache_root_source=dedicated_mount"
  fi

  if [[ -d "$RUST_MOUNT" ]]; then
    if rust_mount_healthy; then
      echo "rust_mount_status=mounted"
    elif mountpoint -q "$RUST_MOUNT"; then
      echo "rust_mount_status=stale-mounted"
    else
      echo "rust_mount_status=not-mounted"
    fi
  else
    echo "rust_mount_status=missing"
  fi
  echo "auto_attach_enabled=${CODEX_AUTO_ATTACH_RUST_DISK:-1}"
  echo "auto_attach_script=${CODEX_RUST_ATTACH_SCRIPT:-$RUST_ATTACH_SCRIPT_DEFAULT}"
}

show_sizes() {
  resolve_cache_paths
  show_root
  du -sh "$CACHE_BASE" "$FAST_TARGET_DIR" "$FULL_TEST_TARGET_DIR" 2>/dev/null || true
}

session_start() {
  ensure_dirs
  echo "session-start: pruning previous full-test lane..."
  rm -rf "$FULL_TEST_TARGET_DIR"
  mkdir -p "$FULL_TEST_TARGET_DIR"
  show_sizes
}

run_fast() {
  ensure_dirs
  (
    cd "$CODEX_RS_DIR"
    export CARGO_TARGET_DIR="$FAST_TARGET_DIR"
    export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-line-tables-only}"
    export CARGO_PROFILE_DEV_INCREMENTAL="${CARGO_PROFILE_DEV_INCREMENTAL:-true}"
    export CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
    export CARGO_PROFILE_TEST_INCREMENTAL="${CARGO_PROFILE_TEST_INCREMENTAL:-true}"

    if [[ $# -eq 0 ]]; then
      set -- check -p codex-cli
    fi

    echo "mode=fast target_dir=$CARGO_TARGET_DIR"
    cargo "$@"
  )
}

run_full_test() {
  ensure_dirs
  (
    cd "$CODEX_RS_DIR"
    export CARGO_TARGET_DIR="$FULL_TEST_TARGET_DIR"
    export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
    export CARGO_PROFILE_DEV_INCREMENTAL="${CARGO_PROFILE_DEV_INCREMENTAL:-false}"
    export CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
    export CARGO_PROFILE_TEST_INCREMENTAL="${CARGO_PROFILE_TEST_INCREMENTAL:-false}"
    export CARGO_INCREMENTAL=0

    if [[ $# -eq 0 ]]; then
      set -- test
    fi

    echo "mode=full-test target_dir=$CARGO_TARGET_DIR"
    cargo "$@"
  )
}

command="${1:-}"
if [[ -z "$command" ]]; then
  usage
  exit 1
fi
shift || true

case "$command" in
  ensure-mount)
    ensure_rust_mount
    ;;
  fast)
    run_fast "$@"
    ;;
  full-test)
    run_full_test "$@"
    ;;
  session-start)
    session_start
    ;;
  root)
    show_root
    ;;
  sizes)
    show_sizes
    ;;
  prune-fast)
    resolve_cache_paths
    rm -rf "$FAST_TARGET_DIR"
    show_sizes
    ;;
  prune-full-test)
    resolve_cache_paths
    rm -rf "$FULL_TEST_TARGET_DIR"
    show_sizes
    ;;
  prune-all)
    resolve_cache_paths
    rm -rf "$CACHE_BASE"
    show_sizes
    ;;
  *)
    usage
    exit 1
    ;;
esac

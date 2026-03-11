#!/bin/sh

set -eu

usage() {
  cat >&2 <<'EOF'
Usage: scripts/rebase-bolt-on-release.sh [latest|rust-vX.Y.Z|vX.Y.Z|X.Y.Z]

Rebases the current branch from its current merged `rust-v*` release tag onto a
newer Codex release tag from the `upstream` remote.

This is intended for a small linear "bolt-on" customization branch that sits on
top of an official release tag.
EOF
  exit 1
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf '%s is required.\n' "$1" >&2
    exit 1
  fi
}

have_command() {
  command -v "$1" >/dev/null 2>&1
}

normalize_tag() {
  case "${1:-latest}" in
    "" | latest)
      printf 'latest\n'
      ;;
    rust-v*)
      printf '%s\n' "$1"
      ;;
    v*)
      printf 'rust-%s\n' "$1"
      ;;
    *)
      printf 'rust-v%s\n' "$1"
      ;;
  esac
}

ensure_clean_worktree() {
  if [ -n "$(git status --porcelain)" ]; then
    printf 'Working tree must be clean before rebasing a bolt-on branch.\n' >&2
    exit 1
  fi
}

resolve_latest_tag() {
  latest_tag="$(git tag --list 'rust-v*' --sort=-version:refname | head -n 1)"
  if [ -z "$latest_tag" ]; then
    printf 'Could not find any rust-v* tags after fetching upstream.\n' >&2
    exit 1
  fi
  printf '%s\n' "$latest_tag"
}

refresh_workspace_lockfile_versions() {
  workspace_manifest="codex-rs/Cargo.toml"
  lockfile="codex-rs/Cargo.lock"

  if [ ! -f "$workspace_manifest" ] || [ ! -f "$lockfile" ]; then
    return 0
  fi

  if ! have_command cargo; then
    printf 'Note: skipping Cargo.lock workspace version refresh because cargo is unavailable.\n' >&2
    return 0
  fi

  if ! have_command python3; then
    printf 'Note: skipping Cargo.lock workspace version refresh because python3 is unavailable.\n' >&2
    return 0
  fi

  printf '==> Refreshing workspace package versions in `%s`\n' "$lockfile"
  metadata_file="$(mktemp)"
  cargo metadata --manifest-path "$workspace_manifest" --format-version 1 --no-deps >"$metadata_file"
  if ! python3 - "$lockfile" "$metadata_file" <<'PY'
import json
import pathlib
import re
import sys

lockfile = pathlib.Path(sys.argv[1])
metadata_path = pathlib.Path(sys.argv[2])
metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
workspace_versions = {
    package["name"]: package["version"]
    for package in metadata["packages"]
}

text = lockfile.read_text(encoding="utf-8")
package_pattern = re.compile(
    r'(\[\[package\]\]\nname = "([^"]+)"\nversion = ")([^"]+)(")',
    re.MULTILINE,
)

def replace(match: re.Match[str]) -> str:
    prefix, package_name, current_version, suffix = match.groups()
    new_version = workspace_versions.get(package_name)
    if new_version is None or new_version == current_version:
        return match.group(0)
    return f"{prefix}{new_version}{suffix}"

updated = package_pattern.sub(replace, text)
if updated != text:
    lockfile.write_text(updated, encoding="utf-8")
PY
  then
    rm -f "$metadata_file"
    return 1
  fi
  rm -f "$metadata_file"
}

require_command git

if [ "$#" -gt 1 ]; then
  usage
fi

branch="$(git symbolic-ref --quiet --short HEAD || true)"
if [ -z "$branch" ]; then
  printf 'Current HEAD is detached. Check out your bolt-on branch first.\n' >&2
  exit 1
fi

if ! git remote get-url upstream >/dev/null 2>&1; then
  printf 'Missing `upstream` remote. Add the official Codex repo as `upstream` first.\n' >&2
  exit 1
fi

ensure_clean_worktree

if [ "$(git config --bool --get rerere.enabled || true)" != "true" ]; then
  cat >&2 <<'EOF'
Note: git rerere is disabled for this repo.
Enable it once to reuse future conflict resolutions:
  git config --local rerere.enabled true
  git config --local rerere.autoupdate true
EOF
fi

printf '==> Fetching upstream tags\n'
git fetch upstream --tags

target_tag="$(normalize_tag "${1:-latest}")"
if [ "$target_tag" = "latest" ]; then
  target_tag="$(resolve_latest_tag)"
fi

if ! git rev-parse -q --verify "refs/tags/$target_tag" >/dev/null 2>&1; then
  printf 'Target tag `%s` was not found locally after fetching upstream.\n' "$target_tag" >&2
  exit 1
fi

current_base_tag="$(git tag --merged HEAD --list 'rust-v*' --sort=-version:refname | head -n 1)"
if [ -z "$current_base_tag" ]; then
  printf 'Could not infer the current merged rust-v* base tag for `%s`.\n' "$branch" >&2
  exit 1
fi

if [ "$current_base_tag" = "$target_tag" ]; then
  printf 'Branch `%s` is already based on `%s`.\n' "$branch" "$target_tag"
  exit 0
fi

timestamp="$(date +%Y%m%d-%H%M%S)"
safe_branch_name="$(printf '%s\n' "$branch" | sed 's#[ /]#-#g')"
backup_branch="backup/${safe_branch_name}-before-${target_tag#rust-v}-${timestamp}"

printf '==> Creating backup branch `%s`\n' "$backup_branch"
git branch "$backup_branch" HEAD

printf '==> Rebasing `%s` from `%s` onto `%s`\n' "$branch" "$current_base_tag" "$target_tag"
git rebase --onto "$target_tag" "$current_base_tag"

refresh_workspace_lockfile_versions

printf '\nRebase complete.\n'
printf 'Backup branch: %s\n' "$backup_branch"
printf 'Old base tag: %s\n' "$current_base_tag"
printf 'New base tag: %s\n' "$target_tag"

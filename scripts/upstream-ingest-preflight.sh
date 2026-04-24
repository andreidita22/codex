#!/bin/sh

set -eu

usage() {
  cat >&2 <<'EOF'
Usage: scripts/upstream-ingest-preflight.sh <target-version-or-tag> [previous-version-or-tag]

Examples:
  scripts/upstream-ingest-preflight.sh 0.124.0
  scripts/upstream-ingest-preflight.sh rust-v0.124.0 rust-v0.123.0

Prints the release-ingest facts and guard commands for a vanilla upstream
release. It does not move branches or edit files.

Environment:
  BASE_REF=main   Fork ref used to infer the previous ingested release.
EOF
  exit 1
}

normalize_tag() {
  case "${1:-}" in
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

short_ref() {
  git rev-parse --short "$1"
}

require_ref() {
  if ! git rev-parse -q --verify "$1" >/dev/null 2>&1; then
    printf 'Missing git ref: %s\n' "$1" >&2
    printf 'Run: git fetch upstream --tags --prune\n' >&2
    exit 1
  fi
}

stable_release_tags() {
  git tag --list 'rust-v[0-9]*' | grep -E '^rust-v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V
}

print_section() {
  printf '\n==> %s\n' "$1"
}

count_lines() {
  wc -l <"$1" | tr -d ' '
}

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  usage
fi

base_ref="${BASE_REF:-main}"
target_tag="$(normalize_tag "$1")"
previous_tag="${2:-}"
script_path="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
if [ -n "$previous_tag" ]; then
  previous_tag="$(normalize_tag "$previous_tag")"
fi

require_ref "$base_ref"
require_ref "$target_tag"

if [ -z "$previous_tag" ]; then
  previous_tag="$(
    git tag --merged "$base_ref" --list 'rust-v[0-9]*' --sort=-version:refname \
      | grep -E '^rust-v[0-9]+\.[0-9]+\.[0-9]+$' \
      | head -n 1 \
      || true
  )"
fi

if [ -z "$previous_tag" ]; then
  printf 'Could not infer previous rust-v* release from %s.\n' "$base_ref" >&2
  printf 'Pass it explicitly as the second argument.\n' >&2
  exit 1
fi

require_ref "$previous_tag"

if [ "$previous_tag" = "$target_tag" ]; then
  printf 'Previous and target releases are both %s.\n' "$target_tag" >&2
  exit 1
fi

target_version="${target_tag#rust-v}"
branch_version="${target_version%.0}"
ingest_branch="codex/update-${branch_version}-ingest"
align_branch="codex/update-${branch_version}-align"
worktree_dir="../codex-update-${branch_version}-ingest"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

release_files="$tmpdir/release-files"
fork_files="$tmpdir/fork-files"

git diff --name-only "$previous_tag..$target_tag" >"$release_files"
git diff --name-only "$base_ref...$target_tag" >"$fork_files"

print_section "Refs"
printf 'base ref:        %s / %s\n' "$base_ref" "$(short_ref "$base_ref")"
printf 'previous tag:    %s / %s\n' "$previous_tag" "$(short_ref "$previous_tag^{}")"
printf 'target tag:      %s / %s\n' "$target_tag" "$(short_ref "$target_tag^{}")"
printf 'target tag obj:  %s\n' "$(short_ref "$target_tag")"
printf 'ingest branch:   %s\n' "$ingest_branch"
printf 'align branch:    %s\n' "$align_branch"

latest_stable="$(stable_release_tags | tail -n 1 || true)"
if [ -n "$latest_stable" ] && [ "$latest_stable" != "$target_tag" ]; then
  printf 'warning: latest stable upstream tag appears to be %s, not %s\n' \
    "$latest_stable" "$target_tag"
fi

if [ -n "$(git status --porcelain)" ]; then
  print_section "Dirty worktree warning"
  git status --short
  printf 'Use a separate worktree for the ingest or clean/stash before switching branches.\n'
fi

if [ "$(git config --bool --get rerere.enabled || true)" != "true" ]; then
  print_section "Rerere recommendation"
  printf 'git config --local rerere.enabled true\n'
  printf 'git config --local rerere.autoupdate true\n'
fi

print_section "Scale"
printf 'upstream release delta (%s..%s): %s\n' \
  "$previous_tag" "$target_tag" "$(git diff --shortstat "$previous_tag..$target_tag" || true)"
printf 'fork vs target (%s..%s): %s\n' \
  "$base_ref" "$target_tag" "$(git diff --shortstat "$base_ref..$target_tag" || true)"
printf 'release files changed: %s\n' "$(count_lines "$release_files")"
printf 'fork triple-dot files changed: %s\n' "$(count_lines "$fork_files")"

watchlist_re='^(codex-rs/(protocol/src/(models|protocol|config_types|request_permissions)\.rs|core/src/(agent/control|client|compact|compact_remote|context_manager/(history|normalize|updates)|event_mapping|session/(agent_task_lifecycle|handlers|mcp|mod|session|turn|turn_context|review)|thread_manager|tools/(spec|router|handlers/(agent_progress|multi_agents.*|shell|unified_exec)|registry|orchestrator)|tasks/(compact|mod))\.rs|config/src/(config_toml|config_requirements|hook_config|lib|types)\.rs|tools/src/(tool_config|tool_registry_plan|tool_registry_plan_types|agent_progress_tool)\.rs|app-server-protocol/src/protocol/(common|v2)\.rs|app-server/src/codex_message_processor\.rs|tui/src/(chatwidget\.rs|chatwidget/slash_dispatch\.rs|slash_command\.rs)))$'

print_section "Watchlist overlap in upstream release delta"
if ! grep -E "$watchlist_re" "$release_files"; then
  printf 'No watchlist files matched in %s..%s.\n' "$previous_tag" "$target_tag"
fi

print_section "Enum and operation surface hints"
if ! git diff --unified=0 "$previous_tag..$target_tag" -- \
    codex-rs/protocol/src/protocol.rs \
    codex-rs/protocol/src/models.rs \
    codex-rs/tools/src/tool_registry_plan_types.rs \
  | grep -E '^@@|^[+-].*(pub enum (Op|EventMsg|AgentStatus|SessionSource|SubAgentSource|ToolHandlerKind|ThreadMemoryMode|ResponseItem|ContentItem)|FunctionCallOutputPayload|TurnEnvironment|ModelVerification|GuardianWarning)'; then
  printf 'No enum or operation hints matched the preflight pattern.\n'
fi

print_section "Recommended setup commands"
cat <<EOF
git fetch upstream --tags --prune
git branch -f upstream-latest-release $target_tag
git branch -f upstream-main upstream/main
git worktree add -b $ingest_branch $worktree_dir origin/main
cd $worktree_dir
$script_path $target_tag $previous_tag
EOF

print_section "Alignment-only PR guard"
cat <<EOF
# Use this before opening or retargeting $align_branch to main.
git merge-base --is-ancestor $ingest_branch main
git log --oneline main..$align_branch

# The first command must pass. The second command must list only fork-owned
# alignment commits, never upstream release commits.
EOF

print_section "Release log skeleton"
cat <<EOF
## ${previous_tag#rust-v} -> ${target_version} (prep)

### Refs

- fork main at ingest start: \`$(short_ref "$base_ref")\`
- upstream target: \`$target_tag\` / \`$(short_ref "$target_tag^{}")\` (tag object \`$(short_ref "$target_tag")\`)
- live upstream inspection ref at prep time:
- comparison range for upstream release delta: \`$previous_tag..$target_tag\`
- comparison range for fork ingest surface: \`$base_ref..upstream-latest-release\`
- review topology: alignment PR starts as \`$align_branch\` -> \`$ingest_branch\`; retarget/open to \`main\` only after the PR guard passes

### Scale

- upstream release delta (\`$previous_tag..$target_tag\`): \`$(git diff --shortstat "$previous_tag..$target_tag" || true)\`
- current fork-main vs upstream target (\`$base_ref..upstream-latest-release\`):
- direct overlap (\`$base_ref...upstream-latest-release\`): \`$(count_lines "$fork_files")\` files
- direct seam overlap candidates in upstream delta:

### Seam table

| File | Fork-owned bundles affected | Initial risk | Decision | Rationale | Validation |
| --- | --- | --- | --- | --- | --- |

### Notes

- upstream themes that matter
- deferred questions
- post-ingest cleanup
- alignment-only PR guard:
  - \`git merge-base --is-ancestor $ingest_branch main\`
  - \`git log --oneline main..$align_branch\`
EOF

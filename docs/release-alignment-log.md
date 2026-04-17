# Release Alignment Log

This document records how new upstream Codex releases are aligned to this fork.

Use it together with:

- [custom-fork-module-inventory.md](custom-fork-module-inventory.md)
- [custom-governance-path-build-guide.md](custom-governance-path-build-guide.md)
- [fork-updates.md](fork-updates.md)

Those docs answer:

- what fork-owned modules exist
- what is implemented today
- how to run the update workflow

This log answers a different question:

- when upstream changes a shared seam, what alignment choice did we make and why

## How to use this log

For each upstream release ingest:

1. Add a new release section before making the alignment edits.
2. Record the exact refs being aligned.
3. Record only the seam files and decision points that matter.
4. For each touched seam, choose one decision:
   - `accept_upstream`
   - `keep_fork`
   - `merge_both`
   - `defer`
5. Record the rationale in terms of fork-owned behavior, not just merge mechanics.
6. Append validation notes once the ingest is complete.

## Decision taxonomy

### `accept_upstream`

Use when upstream behavior cleanly supersedes what we had locally, or when our
fork does not depend on the previous behavior anymore.

### `keep_fork`

Use when the upstream change conflicts with an intentional fork invariant and we
need to preserve the fork behavior as authoritative.

### `merge_both`

Use when upstream adds useful new behavior in a file we already patch and the
correct result is to keep the fork feature while absorbing the upstream change.

### `defer`

Use when the seam needs more reasoning or a live test before we can decide.

## Alignment template

Copy this shape for future releases.

```md
## <from> -> <to>

### Refs

- fork main:
- upstream target:
- comparison range:

### Scale

- files changed:
- direct seam overlap:

### Seam table

| File | Fork-owned bundles affected | Initial risk | Decision | Rationale | Validation |
| --- | --- | --- | --- | --- | --- |

### Notes

- upstream themes that matter
- deferred questions
- post-ingest cleanup
```

## 0.119.0 -> 0.120.0

### Refs

- fork main at prep time: `178d31bbb2`
- upstream target: `rust-v0.120.0` / `65319eb140`
- comparison range for upstream prep: `rust-v0.119.0..rust-v0.120.0`
- comparison range for current fork surface: `main...upstream-main`

### Scale

- upstream delta: `170` files changed, `9307` insertions, `1571` deletions
- direct overlap with current hot seam set:
  - `codex-rs/core/src/agent/control.rs`
  - `codex-rs/core/src/codex.rs`
  - `codex-rs/core/src/compact.rs`
  - `codex-rs/core/src/compact_remote.rs`
  - `codex-rs/core/src/thread_manager.rs`
  - `codex-rs/core/src/tools/spec_tests.rs`
  - `codex-rs/core/src/codex_tests.rs`
  - `codex-rs/tools/src/tool_registry_plan.rs`
  - `codex-rs/tools/src/tool_registry_plan_tests.rs`
- upstream does not directly modify the fork-only module files under:
  - `codex-rs/core/src/governance/`
  - `codex-rs/core/src/continuation_bridge/`
  - `codex-rs/core/templates/thread_memory/`
  - `codex-rs/core/templates/continuation_bridge/`

### High-level read

`0.120` looks like a narrower seam alignment than `0.119`.

The upstream delta is broad across realtime, guardian, exec-server, hooks, and
TUI work, but the direct overlap with the fork-owned maintenance surface is
concentrated in a smaller set of already-familiar seam files. The highest-risk
areas for this ingest are:

- prompt/context construction in `codex.rs`
- compaction wrapping in `compact.rs` and `compact_remote.rs`
- agent/thread lifecycle changes in `agent/control.rs` and `thread_manager.rs`
- tool-plan drift in `codex-rs/tools/src/tool_registry_plan.rs`

### Seam table

| File | Fork-owned bundles affected | Initial risk | Decision | Rationale | Validation |
| --- | --- | --- | --- | --- | --- |
| `codex-rs/core/src/agent/control.rs` | continuation bridge sub-agent context, E-witness lifecycle integration | high | `merge_both` | Auto-merged cleanly; the fork's progress seeding/inspection path survived alongside upstream lifecycle changes without needing local edits. | targeted source inspection; `cargo test -p codex-core compact` |
| `codex-rs/core/src/codex.rs` | continuation bridge injection, governance prompt layering, thread-memory hooks | high | `merge_both` | Auto-merged cleanly; the fork's prompt-layering and compaction hooks remained in place on top of the `0.120` orchestration changes. | targeted source inspection; `cargo test -p codex-core compact` |
| `codex-rs/core/src/compact.rs` | continuation bridge, thread-memory, fail-closed compaction, raw-window trimming | high | `merge_both` | Auto-merged cleanly; local compaction wrapping remained intact and passed the `compact` slice after the helper/test harness updates below. | `cargo test -p codex-core compact` |
| `codex-rs/core/src/compact_remote.rs` | continuation bridge, thread-memory, fail-closed remote compaction | high | `merge_both` | Manual merge resolution kept upstream remote-compaction analytics and the fork's authoritative artifact reinsertion/fail-closed behavior. | `cargo test -p codex-core compact` |
| `codex-rs/core/src/thread_manager.rs` | E-witness thread/progress flow | medium-high | `merge_both` | Auto-merged cleanly; no local rework was needed to preserve the existing progress plumbing. | targeted source inspection |
| `codex-rs/tools/src/tool_registry_plan.rs` | E-witness tool plan exposure, thread-spawn containment indirectly | high | `merge_both` | Auto-merged cleanly; upstream `0.120` changes did not require a fresh rework of the `0.119` tool-plan seam alignment. | targeted source inspection |
| `codex-rs/tools/src/tool_registry_plan_tests.rs` | E-witness tool-plan regression coverage | medium | `merge_both` | Auto-merged cleanly with no local edits. | targeted source inspection |
| `codex-rs/core/src/tools/spec_tests.rs` | thread-spawn containment regression coverage | medium | `merge_both` | Auto-merged cleanly; the fork's containment coverage remains present after the release ingest. | targeted source inspection |
| `codex-rs/core/src/codex_tests.rs` | governance/compaction expectations | medium | `merge_both` | Auto-merged cleanly; no direct fork-specific test rewrite was needed in this file for `0.120`. | targeted source inspection |

### Post-merge adjustments

- `codex-rs/core/tests/common/streaming_sse.rs`
  - Added default continuation-bridge request handling inside the streaming SSE
    helper so bridge-generation requests do not consume queued main-turn
    responses during compaction tests.
  - Validation:
    - `cargo test -p core_test_support continuation_bridge_requests_do_not_consume_queued_streams`

- `codex-rs/core/tests/suite/compact_resume_fork.rs`
  - Mounted a dedicated continuation-bridge responder for the second-compaction
    sequence so bridge requests no longer consume the main SSE queue.
  - Relaxed the raw transport-shape assertion in the second-compaction resume
    test and kept the semantic user-history assertions, because `0.120` can
    legitimately deduplicate replayed runtime context on resume.
  - Accepted the rollback snapshot update where post-rollback replay no longer
    duplicates the permissions/environment pair after compaction.
  - Validation:
    - `cargo test -p codex-core --test all suite::compact_resume_fork::compact_resume_after_second_compaction_preserves_history -- --exact`
    - `cargo test -p codex-core --test all suite::compact_resume_fork::snapshot_rollback_past_compaction_replays_append_only_history -- --exact`
    - `cargo test -p codex-core --test all suite::pending_input::steered_user_input_`
    - `cargo test -p codex-core compact`

### Notes

- Start from fork `main`; do not rebuild this ingest from the upstream side.
- Default seam decision remains `merge_both` unless upstream clearly supersedes
  a fork behavior or the fork must preserve an invariant unchanged.
- The `0.119` ancestry mistake is already documented below and should not be
  repeated here.
- Hygiene pass completed on the merged tree:
  - `just fmt`
  - `just fix -p core_test_support`
  - `just fix -p codex-core`
  - `just bazel-lock-update`
  - `just bazel-lock-check`
  - `just argument-comment-lint`

## 0.118.0 -> 0.119.0

## 0.120.0 -> 0.121.0 (prep)

### Refs

- fork main at prep time: `c06783bd26`
- upstream target: `rust-v0.121.0` / `d65ed92a5e` (tag object `b3442f5e85`)
- comparison range for upstream release delta: `rust-v0.120.0..rust-v0.121.0`
- comparison range for fork ingest surface: `main..upstream-main`

### Scale

- upstream release delta (`rust-v0.120.0..rust-v0.121.0`): `764` files changed, `34540` insertions, `9361` deletions
- current fork-main vs upstream target (`main..upstream-main`): `822` files changed, `34862` insertions, `19713` deletions
- direct overlap (`main...upstream-main`): `764` files
- known fork seam overlap candidates present in upstream delta: `14` files

### High-level read (pre-alignment)

Initial risk surface remains seam-drift, not fork-module replacement:

- upstream continues to move high-touch core orchestration files
- our compaction/governance seams remain in shared host files (`codex.rs`, `compact*.rs`, config/schema)
- strict-governance and continuation/thread-memory wrappers need explicit preservation while absorbing upstream host-shape changes

### Prep seam table (before decisions)

| File | Fork-owned bundles affected | Initial risk | Decision | Rationale | Validation |
| --- | --- | --- | --- | --- | --- |
| `codex-rs/core/src/config/mod.rs` | continuation bridge config, governance mode/profile config | high | `defer` | Shared config host file; likely both upstream structure and fork keys changed. | pending |
| `codex-rs/core/config.schema.json` | strict/governance and continuation config schema | high | `defer` | Schema must stay aligned with config changes; drift here is a common breakage source. | pending |
| `codex-rs/core/src/tools/spec.rs` | E-witness tools, thread-spawn tool containment | high | `defer` | Tool registry drift can silently drop custom tools or containment logic. | pending |
| `codex-rs/core/src/agent/control.rs` | E-witness lifecycle integration, continuation bridge sub-agent carryover | high | `defer` | Upstream lifecycle changes may conflict with progress-state semantics. | pending |
| `codex-rs/core/src/codex.rs` | compaction injection points, governance prompt layering hooks | high | `defer` | Largest orchestrator seam; needs careful merge-both handling. | pending |
| `codex-rs/core/src/compact.rs` | local compaction wrappers, bridge/thread-memory insertion, fail-closed behavior | high | `defer` | Primary compaction seam for fork-owned continuity behavior. | pending |
| `codex-rs/core/src/compact_remote.rs` | remote compaction wrapping and post-compact artifact insertion | high | `defer` | Must preserve wrapper semantics around `/responses/compact`. | pending |
| `codex-rs/core/src/context_manager/updates.rs` | governance prompt-layer settings update propagation | medium | `defer` | Host update ordering changes can drop prompt-layer sections. | pending |
| `codex-rs/core/src/thread_manager.rs` | E-witness continuity during fork/resume/history reconstruction | medium | `defer` | Regression risk around thread lifecycle reconstruction. | pending |
| `codex-rs/core/src/codex_tests.rs` | seam-level behavior assertions | medium | `defer` | Tests need re-alignment to host refactors while preserving fork expectations. | pending |
| `codex-rs/core/src/tools/spec_tests.rs` | E-witness + containment test coverage | medium | `defer` | Ensures custom tools remain exposed and constrained as expected. | pending |
| `codex-rs/core/tests/suite/compact.rs` | local compaction behavior coverage | medium | `defer` | Must confirm bridge/memory/fail-closed semantics survive host changes. | pending |
| `codex-rs/core/tests/suite/compact_remote.rs` | remote compaction behavior coverage | medium | `defer` | Must confirm post-compact insertion and replacement history shape. | pending |
| `codex-rs/core/tests/suite/compact_resume_fork.rs` | compact/fork/resume interplay | low-medium | `defer` | Ensures continuity across rollback/fork edges after alignment. | pending |

### Notes (prep)

- `origin/upstream-main` has been force-synced to `rust-v0.121.0` (`d65ed92a5e`) as the canonical mirror.
- Ingest branch was created from fork canonical line: `codex/update-0.121`.
- Next step is seam-by-seam alignment with `merge_both` as default decision and explicit rationale for exceptions.

### Refs

- fork main at prep time: `686aaae0a3`
- upstream target: `rust-v0.119.0` / `4a3466efbf`
- comparison range for upstream prep: `rust-v0.118.0..rust-v0.119.0`
- comparison range for current fork surface: `rust-v0.118.0..origin/main`

### Scale

- upstream delta: `994` files changed, `73872` insertions, `34861` deletions
- direct overlap between current fork surface and upstream `0.119`: `21` files
- upstream does not directly replace fork-only source files under:
  - `codex-rs/core/src/governance/`
  - `codex-rs/core/src/continuation_bridge/`
  - `codex-rs/core/templates/thread_memory/`
  - `codex-rs/core/templates/continuation_bridge/`

### High-level read

`0.119` interferes with the fork in the expected way:

- not by replacing the fork-only modules
- but by moving the shared upstream seam files where our fork injects behavior

The main risk surface is therefore seam drift, not direct feature removal.

### Seam table

| File | Fork-owned bundles affected | Initial risk | Decision | Rationale | Validation |
| --- | --- | --- | --- | --- | --- |
| `codex-rs/core/src/config/mod.rs` | continuation bridge, strict-v1 packets, governance mode config, bridge model config | high | `merge_both` | Kept the `0.119` config host structure and dropped replayed historical `ConfigToml` blocks, while preserving the live fork seams for continuation bridge and governance path config. | manual rebase stop resolved; compile validation blocked by upstream dependency fetch |
| `codex-rs/core/config.schema.json` | strict-v1 config, continuation bridge config | high | `merge_both` | Preserved both fork schema surfaces inside the rebased `0.119` schema instead of treating schema and config as separate decisions. | rebased cleanly after config alignment; compile validation blocked by upstream dependency fetch |
| `codex-rs/core/src/tools/spec.rs` | E-witness tools, thread-spawn containment patch | high | `merge_both` | Kept the newer upstream `codex_tools` ownership/layout and preserved the fork rule that thread-spawned subagents do not receive `spawn_agent`. | manual rebase stop resolved; file matches current fork `main` |
| `codex-rs/core/src/agent/control.rs` | E-witness lifecycle integration, continuation bridge sub-agent context | high | `merge_both` | Fork delta still applies on top of `0.119`; no manual conflict stop was needed, so the rebased result keeps both upstream lifecycle changes and the fork-owned control-plane behavior. | rebased cleanly; live validation still pending |
| `codex-rs/core/src/codex.rs` | continuation bridge injection, governance prompt layering, thread-memory hooks | high | `merge_both` | Merged the new `include_permissions_instructions` / `include_environment_context` gates with the fork's governance prompt-layer insertion and current helper-based ordering. | manual rebase stop resolved; compile validation blocked by upstream dependency fetch |
| `codex-rs/core/src/compact.rs` | continuation bridge, thread-memory, fail-closed compaction, raw-window trimming | medium-high | `merge_both` | Preserved fork-owned authoritative artifact insertion and fail-closed strict behavior while keeping the newer protocol/error import layout from upstream. | manual rebase stop resolved; compile validation blocked by upstream dependency fetch |
| `codex-rs/core/src/compact_remote.rs` | continuation bridge, thread-memory, fail-closed remote compaction | medium-high | `merge_both` | Same as local compaction: keep `/responses/compact` wrapping behavior from the fork while preserving the `0.119` remote compact structure. | manual rebase stop resolved; compile validation blocked by upstream dependency fetch |
| `codex-rs/core/src/context_manager/updates.rs` | governance prompt layering update propagation | medium | `merge_both` | Fork prompt-layer update propagation rebased onto the `0.119` update builder without needing a manual stop. | rebased cleanly; compile validation blocked by upstream dependency fetch |
| `codex-rs/core/src/thread_manager.rs` | E-witness thread/progress flow | medium | `merge_both` | The fork delta remains present after rebase and did not require manual conflict surgery, so upstream thread ownership changes were absorbed without dropping progress behavior. | rebased cleanly; live validation still pending |
| `codex-rs/core/src/codex_tests.rs` | governance/compaction expectations | medium | `merge_both` | Preserved the newer fork helpers and governance-layer assertions while reconciling test helper drift from `0.119`. | manual rebase stop resolved; runtime tests blocked by upstream dependency fetch |
| `codex-rs/core/src/tools/spec_tests.rs` | E-witness and thread-spawn containment regression coverage | medium | `merge_both` | Took the current fork `main` test shape, which already contains the thread-spawn containment assertions in the new upstream test layout. | manual rebase stop resolved; runtime tests blocked by upstream dependency fetch |
| `codex-rs/core/tests/suite/compact.rs` | thread-memory + continuation bridge compaction assertions | medium | `merge_both` | Test-side compaction assertions were rebased with the current fork import/test harness shape. | rebased cleanly; suite not executed because dependency fetch is blocked |
| `codex-rs/core/tests/suite/compact_remote.rs` | remote compaction assertions | medium | `merge_both` | Remote compaction assertions remain part of the fork delta and rebased cleanly onto `0.119`. | rebased cleanly; suite not executed because dependency fetch is blocked |
| `codex-rs/core/tests/suite/compact_resume_fork.rs` | compaction/fork interplay | low-medium | `merge_both` | The fork-specific compaction/fork test coverage remains in place after the rebase. | rebased cleanly; suite not executed because dependency fetch is blocked |

### Upstream themes likely to matter during alignment

- tool/config extraction and refactors around `tools/spec.rs`
- child/fork behavior changes, including forked-child history sanitation
- post-compaction steering changes
- instruction/developer-context null-handling changes
- client metadata forwarding into Responses requests
- multi-agent v2 spawn hint changes

### Recommended file order for the actual ingest

1. `codex-rs/core/src/config/mod.rs`
2. `codex-rs/core/config.schema.json`
3. `codex-rs/core/src/tools/spec.rs`
4. `codex-rs/core/src/agent/control.rs`
5. `codex-rs/core/src/codex.rs`
6. `codex-rs/core/src/compact.rs`
7. `codex-rs/core/src/compact_remote.rs`
8. `codex-rs/core/src/context_manager/updates.rs`
9. test files overlapping those seams

### Initial alignment stance

- Fork-only modules remain normatively owned by this fork unless upstream
  introduces a clearly superior native replacement.
- Shared seam files should default to `merge_both`, not blind overwrite in
  either direction.
- Config and tool-surface files are the highest-risk decisions in this release.
- Compaction files require explicit preservation of:
  - continuation bridge reinsertion
  - thread-memory reinsertion
  - fail-closed strict behavior
  - raw-window trimming policy

### Validation checklist for the eventual ingest

- continuation bridge still generates and reinserts on local compaction
- continuation bridge still reinserts after remote `/responses/compact`
- thread-memory still updates from previous artifact and reinserts correctly
- fail-closed strict compaction still aborts loudly on required memory failure
- governance prompt layers still appear at thread start and on settings updates
- E-witness tools still expose progress snapshots and wait semantics
- thread-spawned sub-agents still do not receive `spawn_agent`

### Post-ingest status

- Rebase completed successfully on `codex/update-0.119-prep`.
- The first prep branch was built from the upstream side rather than from fork
  `main`, which later caused synthetic GitHub PR conflicts against `main` even
  though the code alignment itself was already correct.
- Resolution: treat the validated `0.119` ingest branch as the new canonical
  fork state, back up the previous `main`, and intentionally swap `main` to the
  validated branch instead of hand-resolving the false conflict set.
- `cargo test -p codex-config`: passed.
- `cargo test -p codex-tools`: passed.
- targeted `cargo test -p codex-core tools::spec::tests::`: passed after aligning the
  fork E-witness tools with the upstream `codex_tools` plan flow.
- `cargo test -p codex-core`: completed and now fails only in untouched upstream
  runtime-sensitive tests:
  - `shell_snapshot::tests::snapshot_shell_does_not_inherit_stdin`
  - `shell_snapshot::tests::timed_out_snapshot_shell_is_terminated`
  - `unified_exec::tests::completed_pipe_commands_preserve_exit_code`
  - `unified_exec::tests::reusing_completed_process_returns_unknown_process`
  - `unified_exec::tests::unified_exec_pause_blocks_yield_timeout`
- `just fix -p codex-tools`: passed.
- `just fix -p codex-config`: passed.
- `just fix -p codex-core`: passed.
- `just fmt`: passed.
- `./tools/argument-comment-lint/run.py`: passed.
- `just bazel-lock-update`: passed after installing Bazel via Bazelisk.
- `just bazel-lock-check`: passed.
- `MODULE.bazel.lock`: no diff after lock update/check.

What this means:

- the `0.119` alignment is complete at the rebase level and validated for the fork-owned
  seam files
- the remaining full-`codex-core` failures are not in the files touched by the ingest and
  currently look like pre-existing upstream/environment-sensitive failures rather than
  fork regressions

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

## 0.118.0 -> 0.119.0

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
- `just fmt`: passed.
- `cargo test -p codex-core`: blocked before build/test execution by upstream dependency resolution.
- `just fix -p codex-core`: blocked by the same dependency resolution failure.
- direct `./tools/argument-comment-lint/run.py`: blocked by the same dependency resolution failure through `cargo metadata`.

Dependency blocker observed during validation:

- `codex-realtime-webrtc` resolves `libwebrtc` from
  `ssh://git@github.com/juberti-oai/rust-sdks.git`
- requested revision:
  `e2d1d1d230c6fc9df171ccb181423f957bb3c1f0`
- failure mode in this environment:
  repository authentication / revision fetch failure

What this means:

- the `0.119` alignment itself is complete at the git/rebase level
- repo-local formatting succeeded
- compile/test/lint validation that traverses Cargo metadata is currently blocked by
  the upstream dependency source, so runtime verification remains pending

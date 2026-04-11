# Custom fork module inventory

This document is the maintainer-facing map of the fork-owned surface area that
must be revalidated when ingesting new upstream Codex releases.

It complements:

- [docs/fork-updates.md](fork-updates.md)
- [docs/local-builds.md](local-builds.md)

Those two docs explain the release-ingest workflow. This one explains what the
fork actually adds on top of upstream.

## Scope

The inventory below is derived from the fork-only commit ranges:

- merged fork surface: `upstream/main..origin/main`
- branch-local additions, when present: `origin/main..HEAD`

It intentionally focuses on fork-owned modules and their hot files. It does not
attempt to restate every touched snapshot, lockfile refresh, or upstream
release-merge artifact.

## How to use this during upstream ingest

1. Update `upstream-main`, create an ingest branch from current fork `main`,
   and align the new release using [fork-updates.md](fork-updates.md).
2. Revisit each module listed here, in order.
3. Resolve conflicts in the listed hot files first.
4. Re-run the targeted tests for the affected bundle before doing a wider build.
5. Update this document if a custom bundle is merged, removed, or replaced.

## Bundle summary

| Bundle | Status | Commits | Main seams |
| --- | --- | --- | --- |
| Continuation bridge | merged on `origin/main` | `c96b7f9e5a`, `afac096039`, `ab623fa1d1`, `1088d14864` | `compact.rs`, `compact_remote.rs`, `continuation_bridge.rs`, `config/mod.rs`, templates |
| Update/build workflow helpers | merged on `origin/main` | `bcd9716227`, `a3aefcd26a`, `532ff58d7d`, `909f9df193` | `scripts/build-vscode-binary.sh`, `scripts/prune-build-artifacts.sh`, docs, legacy `scripts/rebase-bolt-on-release.sh` |
| E-witness live sub-agent progress | merged on `origin/main` | `bf5705f1b4` | `agent/progress.rs`, `agent/control.rs`, `tools/spec.rs`, tool handlers/specs |
| strict-v1 p1 packets/compiler | merged on `origin/main` | `4c24a10b79`, `933e19a1e1` | `governance/packets.rs`, `governance/compiler.rs`, `config/mod.rs` |
| strict-v1 p2 transitions/diagnostics | merged on `origin/main` | `b1841b1abe`, `0b54dfec38` | `governance/transitions.rs`, `governance/diagnostics.rs` |
| strict-v1 p3 thread memory | merged on `origin/main` | `a611d96b36`, `62adb98ead` | `governance/thread_memory.rs`, `compact.rs`, `compact_remote.rs`, templates |
| strict-v1 p4 prompt layering | merged on `origin/main` | `6f53f9c9de`, `c287ba8676` | `governance/prompt_layers.rs`, `codex.rs`, `context_manager/updates.rs` |
| strict-v1 p5 fail-closed compaction window | merged on `origin/main` | `e42a21fdbf`, `39232dce39` | `compact.rs`, `compact_remote.rs`, `compact_tests.rs` |
| Thread-spawn sub-agent tool containment | merged on `origin/main` | `4c1ce68a44` | `tools/spec.rs`, `tools/spec_tests.rs` |

## 1. Continuation bridge

### Commits

- `c96b7f9e5a` Add continuation bridge compaction handoff
- `afac096039` Refine continuation bridge handoff
- `ab623fa1d1` Refine continuation bridge variants and local build workflow
- `1088d14864` Restore vanilla subagent environment context path

### What it adds

- A pre-compaction structured handoff artifact inserted back into replacement
  history after compaction.
- Variant support:
  - `baton`
  - `rich_review`
- Optional dedicated model and reasoning overrides for bridge generation.
- Optional prompt override for experimentation without rebuilding templates.
- Coarse sub-agent carryover block for continuity across compaction.

### Runtime hot files

- [codex-rs/core/src/continuation_bridge.rs](../codex-rs/core/src/continuation_bridge.rs)
- [codex-rs/core/src/continuation_bridge/baton.rs](../codex-rs/core/src/continuation_bridge/baton.rs)
- [codex-rs/core/src/continuation_bridge/rich_review.rs](../codex-rs/core/src/continuation_bridge/rich_review.rs)
- [codex-rs/core/src/continuation_bridge/subagent_context.rs](../codex-rs/core/src/continuation_bridge/subagent_context.rs)
- [codex-rs/core/src/compact.rs](../codex-rs/core/src/compact.rs)
- [codex-rs/core/src/compact_remote.rs](../codex-rs/core/src/compact_remote.rs)
- [codex-rs/core/src/codex.rs](../codex-rs/core/src/codex.rs)
- [codex-rs/core/src/config/mod.rs](../codex-rs/core/src/config/mod.rs)
- [codex-rs/core/src/agent/control.rs](../codex-rs/core/src/agent/control.rs)

### Templates and schema artifacts

- [codex-rs/core/templates/continuation_bridge/variants/baton/prompt.md](../codex-rs/core/templates/continuation_bridge/variants/baton/prompt.md)
- [codex-rs/core/templates/continuation_bridge/variants/baton/schema.json](../codex-rs/core/templates/continuation_bridge/variants/baton/schema.json)
- [codex-rs/core/templates/continuation_bridge/variants/rich_review/prompt.md](../codex-rs/core/templates/continuation_bridge/variants/rich_review/prompt.md)
- [codex-rs/core/templates/continuation_bridge/variants/rich_review/schema.json](../codex-rs/core/templates/continuation_bridge/variants/rich_review/schema.json)

### Config surface

- `continuation_bridge_prompt`
- `continuation_bridge_prompt_file`
- `continuation_bridge_variant`
- `continuation_bridge_model`
- `continuation_bridge_reasoning_effort`

### Upstream conflict risk

- High in compaction codepaths and turn-context plumbing.
- Medium in config parsing/schema whenever upstream adds config fields or
  refactors `Config`.
- Medium in agent-control code when sub-agent metadata flow changes.

### Revalidation focus after ingest

- Manual compaction path still injects `<continuation_bridge ...>` after
  compaction.
- Remote compaction path still injects `<continuation_bridge ...>` after
  `/responses/compact`.
- Variant selection, prompt override, and bridge-model override still resolve
  correctly.

## 2. Update/build workflow helpers

### Commits

- `bcd9716227` Add bolt-on release rebase helper
- `a3aefcd26a` Refresh lockfile versions during release rebases
- `532ff58d7d` Refresh 0.115.0 lockfile and compaction snapshots
- `909f9df193` Sync lockfile and remove stale custom_prompts export

### What it adds

- Stable local build helpers for the VS Code binary path.
- Artifact pruning helper tuned for this fork’s Rust build workflow.
- Maintainer docs for the current fork-as-canonical update/build loop.
- Legacy release-rebase helper retained only for the older small-patch workflow.

### Main files

- [scripts/rebase-bolt-on-release.sh](../scripts/rebase-bolt-on-release.sh)
- [scripts/build-vscode-binary.sh](../scripts/build-vscode-binary.sh)
- [scripts/prune-build-artifacts.sh](../scripts/prune-build-artifacts.sh)
- [docs/fork-updates.md](fork-updates.md)
- [docs/local-builds.md](local-builds.md)

### Upstream conflict risk

- Low at runtime.
- Medium whenever upstream changes workspace layout, package names, or release
  build commands.

### Revalidation focus after ingest

- Active update docs still describe the current fork-as-canonical ingest flow.
- VS Code binary script still copies the correct executable to the stable
  wrapper path.
- Prune script still matches the active build strategy.
- If the legacy rebase helper is still kept, it remains clearly documented as
  legacy-only and is not mistaken for the mainline update path.

## 3. E-witness live sub-agent progress

### Commit

- `bf5705f1b4` Add E-witness agent progress inspection and wait tools

### What it adds

- Two collab tools:
  - `inspect_agent_progress`
  - `wait_for_agent_progress`
- A live agent-progress reducer and registry for sub-agent execution phases.
- Spawn-state and first-progress separation for launch-failure detection.
- Material-progress wait primitive for orchestration loops.

### Runtime hot files

- [codex-rs/core/src/agent/progress.rs](../codex-rs/core/src/agent/progress.rs)
- [codex-rs/core/src/agent/control.rs](../codex-rs/core/src/agent/control.rs)
- [codex-rs/core/src/thread_manager.rs](../codex-rs/core/src/thread_manager.rs)
- [codex-rs/core/src/codex.rs](../codex-rs/core/src/codex.rs)
- [codex-rs/core/src/tools/handlers/agent_progress.rs](../codex-rs/core/src/tools/handlers/agent_progress.rs)
- [codex-rs/core/src/tools/spec.rs](../codex-rs/core/src/tools/spec.rs)
- [codex-rs/tools/src/agent_progress_tool.rs](../codex-rs/tools/src/agent_progress_tool.rs)

### Upstream conflict risk

- High in agent lifecycle event flow.
- High in tool registration/spec wiring for collaboration mode.
- Medium in thread-manager state ownership.

### Revalidation focus after ingest

- `inspect_agent_progress` still returns coherent snapshots.
- `wait_for_agent_progress` still matches on phase/seq transitions.
- Spawned children still emit progress events into the reducer.

## 4. strict-v1 p1 packets/compiler scaffolding

### Commits

- `4c24a10b79` strict-v1 p1 packet scaffolding
- `933e19a1e1` governance compiler: improve duplicate-layer diagnostics

### What it adds

- `GovernancePathVariant` modes:
  - `off`
  - `strict_v1_shadow`
  - `strict_v1_enforce`
- Typed packet/compiler scaffolding for constitutional prompting.
- Projection/provenance-style compiler diagnostics for malformed layer inputs.

### Runtime hot files

- [codex-rs/core/src/governance/packets.rs](../codex-rs/core/src/governance/packets.rs)
- [codex-rs/core/src/governance/compiler.rs](../codex-rs/core/src/governance/compiler.rs)
- [codex-rs/core/src/governance/mod.rs](../codex-rs/core/src/governance/mod.rs)
- [codex-rs/core/src/config/mod.rs](../codex-rs/core/src/config/mod.rs)

### Upstream conflict risk

- Medium in config plumbing.
- Medium in any future upstream governance/prompt-layer abstraction.

### Revalidation focus after ingest

- `governance_path_variant` still parses from config/schema.
- Compiler still rejects duplicate or malformed layers cleanly.

## 5. strict-v1 p2 transition legality and diagnostics

### Commits

- `b1841b1abe` strict-v1 p2: add transition legality checks and diagnostics
- `0b54dfec38` strict-v1 p2: tighten transition continuity checks

### What it adds

- Transition legality model for:
  - spawn
  - compact
  - resume
  - swap
- Diagnostics taxonomy for governance-path validation failures.

### Runtime hot files

- [codex-rs/core/src/governance/transitions.rs](../codex-rs/core/src/governance/transitions.rs)
- [codex-rs/core/src/governance/diagnostics.rs](../codex-rs/core/src/governance/diagnostics.rs)
- [codex-rs/core/src/governance/compiler.rs](../codex-rs/core/src/governance/compiler.rs)

### Upstream conflict risk

- Low to medium until the legality engine is wired more deeply into runtime
  execution paths.
- Medium if upstream introduces its own typed transition model.

### Revalidation focus after ingest

- Transition tests still pass.
- Diagnostic classes remain stable and readable.

## 6. strict-v1 p3 thread memory

### Commits

- `a611d96b36` Add strict-v1 thread memory compaction artifact
- `62adb98ead` Address PR feedback on thread-memory compaction delta

### What it adds

- ODEU-thread-memory artifact generation during compaction.
- Thread-memory templates and schema.
- Reinjection of `<thread_memory ...>` into compacted replacement history when
  strict governance mode is active.

### Runtime hot files

- [codex-rs/core/src/governance/thread_memory.rs](../codex-rs/core/src/governance/thread_memory.rs)
- [codex-rs/core/src/compact.rs](../codex-rs/core/src/compact.rs)
- [codex-rs/core/src/compact_remote.rs](../codex-rs/core/src/compact_remote.rs)
- [codex-rs/core/src/codex.rs](../codex-rs/core/src/codex.rs)
- [codex-rs/core/templates/thread_memory/prompt.md](../codex-rs/core/templates/thread_memory/prompt.md)
- [codex-rs/core/templates/thread_memory/schema.json](../codex-rs/core/templates/thread_memory/schema.json)

### Upstream conflict risk

- High in compaction paths.
- Medium in memory/state-db interfaces if upstream changes thread-memory or
  memory-pollution handling.

### Revalidation focus after ingest

- Strict modes still inject `thread_memory`.
- Previous memory is reused/updated rather than duplicated.
- The artifact still coexists correctly with the continuation bridge.

## 7. strict-v1 p4 prompt layering

### Commits

- `6f53f9c9de` strict-v1 p4: add governance prompt layering semantics
- `c287ba8676` strict-v1 p4: fix governance layering update ordering and presence

### What it adds

- Constitutional/role/task/runtime prompt-layer synthesis.
- Projection witness-like tagged prompt block inserted into live context.
- Update-path propagation so the same layered prompt survives settings updates,
  not just initial thread construction.

### Runtime hot files

- [codex-rs/core/src/governance/prompt_layers.rs](../codex-rs/core/src/governance/prompt_layers.rs)
- [codex-rs/core/src/codex.rs](../codex-rs/core/src/codex.rs)
- [codex-rs/core/src/context_manager/updates.rs](../codex-rs/core/src/context_manager/updates.rs)
- [codex-rs/core/templates/governance/prompt_layering.md](../codex-rs/core/templates/governance/prompt_layering.md)

### Upstream conflict risk

- High in prompt construction and update ordering.
- High in any future upstream changes to base/developer instruction injection.

### Revalidation focus after ingest

- `<governance_prompt_layers ...>` still appears at thread start.
- The same block still appears after runtime settings/context updates.
- Prompt-layer ordering still matches constitutional -> role -> task -> runtime.

## 8. strict-v1 p5 fail-closed thread memory and raw-window trimming

### Commits

- `e42a21fdbf` Fail-closed governed thread memory and trim raw compaction history
- `39232dce39` Emit local compact error event for fail-closed thread memory

### What it adds

- Fail-closed behavior for required `thread_memory` generation.
- Explicit local compaction error event emission when required thread-memory
  generation fails.
- Trimmed raw retained conversation window so strict compaction keeps less
  preserved verbatim history around the authoritative artifacts.

### Runtime hot files

- [codex-rs/core/src/compact.rs](../codex-rs/core/src/compact.rs)
- [codex-rs/core/src/compact_remote.rs](../codex-rs/core/src/compact_remote.rs)
- [codex-rs/core/src/compact_tests.rs](../codex-rs/core/src/compact_tests.rs)

### Upstream conflict risk

- Very high in compaction paths because this bundle changes failure semantics,
  not just additive artifacts.

### Revalidation focus after ingest

- Strict compaction fails loudly when required thread-memory generation fails.
- Local compaction emits an explicit error event before aborting.
- Raw retained history is reduced only when thread-memory is present.

## 9. Thread-spawn sub-agent tool containment

### Commit

- `4c1ce68a44` Contain thread-spawned subagent tool surface

### What it adds

- Hides `spawn_agent` from thread-spawned sub-agents so they keep collab
  observability/control tools without being able to recursively delegate.
- Preserves the rest of the sub-agent collaboration surface:
  - `inspect_agent_progress`
  - `wait_for_agent_progress`
  - wait/send/close agent tools appropriate to the v1/v2 collaboration mode

### Runtime hot files

- [codex-rs/core/src/tools/spec.rs](../codex-rs/core/src/tools/spec.rs)
- [codex-rs/core/src/tools/spec_tests.rs](../codex-rs/core/src/tools/spec_tests.rs)

### Upstream conflict risk

- Medium to high in tool-surface construction for collaboration mode.
- Medium if upstream changes how thread-spawn session sources are modeled.

### Revalidation focus after ingest

- Thread-spawned children still get E-witness tools.
- Thread-spawned children do not get `spawn_agent`.
- Root/session agents still expose `spawn_agent` normally.

## Hot files across multiple bundles

These are the files most likely to conflict again on future upstream ingests:

- [codex-rs/core/src/compact.rs](../codex-rs/core/src/compact.rs)
- [codex-rs/core/src/compact_remote.rs](../codex-rs/core/src/compact_remote.rs)
- [codex-rs/core/src/codex.rs](../codex-rs/core/src/codex.rs)
- [codex-rs/core/src/config/mod.rs](../codex-rs/core/src/config/mod.rs)
- [codex-rs/core/config.schema.json](../codex-rs/core/config.schema.json)
- [codex-rs/core/src/tools/spec.rs](../codex-rs/core/src/tools/spec.rs)
- [codex-rs/core/src/agent/control.rs](../codex-rs/core/src/agent/control.rs)
- [codex-rs/core/src/context_manager/updates.rs](../codex-rs/core/src/context_manager/updates.rs)

## Minimal post-ingest checklist

After ingesting a new upstream release, verify at minimum:

1. Continuation bridge still generates and reinjects correctly for both local
   and remote compaction.
2. E-witness tools still appear in collab mode and report live sub-agent
   progress coherently.
3. `strict_v1_shadow` still injects both governance prompt layers and
   thread-memory artifacts.
4. Fail-closed compaction still aborts correctly and surfaces a local error
   event.
5. Thread-spawned sub-agents still keep observability tools but not
   `spawn_agent`.
6. The local helper scripts in `scripts/` still match the actual build/update
   workflow of the workspace.

## Commands used to refresh this inventory

```sh
git log --reverse --oneline upstream/main..origin/main
git log --reverse --oneline origin/main..HEAD
```

For a bundle-level file list:

```sh
git show --format= --name-only <commit>
```

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
7. If a later PR branch adds fork-only follow-up commits, record those as a
   separate post-ingest alignment note instead of mixing them into the raw
   upstream-ingest seam table.
8. Before opening or retargeting an alignment PR to `main`, record that the
   alignment-only PR guard passed.

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
- review topology:

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
- alignment-only PR guard:
  - `git merge-base --is-ancestor codex/update-<ver>-ingest main`
  - `git log --oneline main..codex/update-<ver>-align`
```

## 0.124.0 -> 0.125.0 (prep + ingest snapshot)

### Refs

- fork main at ingest start: `26d9ee0501`
- upstream target: `rust-v0.125.0` / `637f7dd6d7` (tag object `7d8152a5d7`)
- live upstream inspection ref at prep time: `upstream-main` / `1c3287125f`
- comparison range for upstream release delta: `rust-v0.124.0..rust-v0.125.0`
- comparison range for fork ingest surface: `origin/main..upstream-latest-release`
- review topology: Path B; land validated raw ingest into `main` first, then create
  `codex/update-0.125-align` from updated `main`

### Scale

- upstream release delta (`rust-v0.124.0..rust-v0.125.0`): `398` files changed,
  `19229` insertions, `6448` deletions
- current fork-main vs upstream target (`origin/main..upstream-latest-release`):
  `561` files changed, `20216` insertions, `46625` deletions
- direct overlap (`origin/main...upstream-latest-release`): `398` files
- direct seam overlap candidates in upstream delta: `27` files

### High-level read (pre-alignment)

`0.125` focuses on app-server transport/thread APIs, permission-profile
round-tripping, model-provider ownership, rollout tracing, plugin management,
and exec/runtime hardening. The highest-risk fork seams are:

- session construction and turn routing, where upstream permission-profile and
  environment-selection changes overlap with fork-owned governance prompt
  layering, continuation bridge, context maintenance, and observability wiring
- app-server protocol/schema churn, where fork-owned refresh/prune methods must
  remain in the generated v2 surface
- config/schema changes, where fork-owned maintenance and compaction config
  must remain serializable and documented
- tool/runtime routing, where upstream MCP, shell, unified-exec, and plugin
  changes must not bypass fork-owned observability or containment paths

### Seam table

| File | Fork-owned bundles affected | Initial risk | Decision | Rationale | Validation |
| --- | --- | --- | --- | --- | --- |
| `codex-rs/Cargo.toml` and `codex-rs/Cargo.lock` | workspace membership and fork crate versions | high | `merge_both` | Preserved fork-owned workspace crates while accepting the 0.125 workspace version and upstream dependency graph. | `cargo check -p codex-core -p codex-app-server-client`; `cargo test -p codex-config -p codex-tools`; `just bazel-lock-update`; `just bazel-lock-check` |
| `codex-rs/app-server-protocol/src/protocol/{common.rs,v2.rs}` and generated schemas | app-server refresh/prune methods and v2 operation surface | high | `merge_both` | Accepted upstream Unix socket, sticky environment, permission-profile, and marketplace API growth while keeping fork refresh/prune methods in Rust and generated TS/JSON schemas. | `just write-app-server-schema`; `cargo test -p codex-app-server-protocol --test schema_fixtures`; exact refresh/prune app-server tests |
| `codex-rs/core/src/session/{handlers.rs,mod.rs,session.rs,turn_context.rs}` | governance prompt layering, continuation bridge, context maintenance, observability adapters | very high | `merge_both` | Preserved fork-owned `ProgressObservation` event routing while accepting upstream live-thread persistence, permission-profile, environment-selection, MCP auth, and rollout-trace event recording. | `cargo check -p codex-core -p codex-app-server-client`; `cargo test -p codex-core compact --lib`; `cargo test -p codex-core context_maintenance --lib` |
| `codex-rs/core/src/{lib.rs,compact_remote.rs,thread_manager.rs,tasks/mod.rs}` | exported fork modules, remote compaction, task lifecycle, thread memory | high | `merge_both` | Kept fork-owned modules exported and kept continuation/thread-memory behavior while absorbing upstream rollout trace and remote thread store changes. | `cargo check -p codex-core -p codex-app-server-client`; core compaction/context-maintenance tests |
| `codex-rs/config/src/{config_toml.rs,lib.rs}` and `codex-rs/core/config.schema.json` | fork config gates, compaction engine config, auto-review compatibility | medium-high | `merge_both` | Accepted upstream config/profile changes without dropping fork-owned config fields or schema entries. | `just write-config-schema`; `cargo test -p codex-config -p codex-tools` |
| `codex-rs/core/src/tools/**` and `codex-rs/core/src/agent/control.rs` | E-witness tool exposure, subagent containment, observability event paths | high | `merge_both` | Accepted upstream MCP/shell/unified-exec/plugin movement while preserving fork-owned tool observations and containment. | `cargo test -p codex-tools`; `cargo test -p codex-core tools::spec::tests --lib` |
| `codex-rs/protocol/src/{models.rs,protocol.rs,request_permissions.rs}` | protocol carriers consumed by fork adapters | high | `accept_upstream` | Upstream protocol additions were additive for this ingest; fork adapters compiled without exhaustiveness changes. | `cargo check -p codex-core -p codex-app-server-client`; schema fixture tests |
| `codex-rs/tui/src/chatwidget.rs` | user-visible command/status rendering for fork maintenance flows | medium | `accept_upstream` | No fork-only TUI conflict was needed during raw ingest; this remains a post-ingest alignment watch point. | `just fmt` |

### Notes

- Local mirror refs were updated before ingest:
  - `upstream-latest-release` -> `rust-v0.125.0` / `637f7dd6d7`
  - `upstream-main` -> `upstream/main` / `1c3287125f`
- Current ingest branch: `codex/update-0.125-ingest`.
- `rust-v0.126.0-alpha.1` exists upstream but is intentionally out of scope for
  this stable-release ingest.
- Merge conflicts resolved in `codex-rs/Cargo.toml`,
  `codex-rs/app-server-protocol/schema/typescript/ClientRequest.ts`,
  `codex-rs/core/src/lib.rs`, `codex-rs/core/src/session/handlers.rs`,
  `codex-rs/core/src/session/mod.rs`, `codex-rs/core/src/session/session.rs`,
  and `codex-rs/core/src/session/turn_context.rs`.
- Validation:
  - `cargo check -p codex-core -p codex-app-server-client`
  - `just write-app-server-schema`
  - `just write-config-schema`
  - `just fmt`
  - `cargo test -p codex-app-server-protocol --test schema_fixtures`
  - `cargo test -p codex-config -p codex-tools`
  - `cargo test -p codex-core compact --lib`
  - `cargo test -p codex-core context_maintenance --lib`
  - `cargo test -p codex-core tools::spec::tests --lib`
  - `cargo test -p codex-app-server suite::v2::compaction::thread_refresh_start_triggers_compaction_and_returns_empty_response --test all -- --exact`
  - `cargo test -p codex-app-server suite::v2::compaction::thread_prune_start_triggers_compaction_and_returns_empty_response --test all -- --exact`
  - `just bazel-lock-update`
  - `just bazel-lock-check`
- Alignment-only PR guard before any later PR to `main`:
  - `git merge-base --is-ancestor codex/update-0.125-ingest main`
  - `git log --oneline main..codex/update-0.125-align`

### Post-ingest alignment note

The `codex/update-0.125-align` pass added focused verification for the
fork-owned seams identified above rather than changing runtime behavior.

- A1 added request-level coverage proving strict governance prompt layers are
  visible in initial and settings-update request payloads, and omitted when
  governance is disabled.
- A2 added remote-compaction coverage proving strict governance reinserts fresh
  thread memory before calling `/responses/compact`, and fails closed without
  calling compact when thread-memory refresh fails.
- A3 added trace/observability coverage proving compatibility-only raw protocol
  events are still recorded in rollout traces without advancing the live
  progress reducer.
- A4 added final tool-registry coverage proving thread-spawn sub-agents keep
  observability and coordination tools while omitting spawn/resume/user-input
  tools.
- `custom-fork-module-inventory.md` was left unchanged because this pass did
  not add, remove, or replace a fork-owned runtime bundle.

Validation added by the post-ingest alignment pass:

- `just fmt`
- `cargo test -p codex-core governance --test all`
- `cargo test -p codex-core governance::prompt_layers --lib`
- `cargo test -p codex-core remote_compact_with_strict_governance_reinserts_thread_memory --test all`
- `cargo test -p codex-core strict_remote_compact_thread_memory_failure_skips_compact_endpoint --test all`
- `cargo test -p codex-core remote_compact_does_not_issue_bridge_request_when_override_is_configured --test all`
- `cargo test -p codex-core compaction_policy_matrix --lib`
- `cargo test -p codex-context-maintenance-policy thread_memory`
- `cargo test -p codex-core compatibility_only_protocol_events_are_traced_without_progress_observation --lib`
- `cargo test -p codex-core inspect_agent_progress_reports_reasoning_phase_for_live_subagent --lib`
- `cargo test -p codex-core wait_for_agent_progress --lib`
- `cargo test -p codex-core session_send_event_observes_primary_item_started_once_with_legacy_echoes --lib`
- `cargo test -p codex-core observed_raw --lib`
- `cargo test -p codex-tools agent_progress_tool`
- `cargo test -p codex-tools thread_spawn_subagents --lib`
- `cargo test -p codex-tools memory_consolidation_subagents_do_not_receive_collaboration_tools --lib`
- `cargo test -p codex-tools test_build_specs_agent_job_worker_tools_enabled --lib`
- `cargo test -p codex-tools subagents_keep_request_user_input_mode_config_and_agent_jobs_workers_opt_in_by_label --lib`
- `git diff --check`

Alignment-only PR guard re-run on 2026-04-25:

- `git merge-base --is-ancestor codex/update-0.125-ingest main`
- `git log --oneline main..codex/update-0.125-align`

At guard time the second command listed no inherited upstream commits and no
alignment commits yet; the branch-local work remained uncommitted.

## 0.123.0 -> 0.124.0 (prep + ingest snapshot)

### Refs

- fork main at ingest start: `dfe2001199`
- upstream target: `rust-v0.124.0` / `e9fb49366c` (tag object `e93a08390b`)
- live upstream inspection ref at prep time: `upstream-main` / `0db6811b7c`
- comparison range for upstream release delta: `rust-v0.123.0..rust-v0.124.0`
- comparison range for fork ingest surface: `main..upstream-latest-release`
- review topology: Path B; land validated raw ingest into `main` first, then create
  any `codex/update-0.124-align` review branch from updated `main`

### Scale

- upstream release delta (`rust-v0.123.0..rust-v0.124.0`): `599` files changed,
  `43224` insertions, `10946` deletions
- current fork-main vs upstream target (`main..upstream-latest-release`): `743`
  files changed, `44110` insertions, `50878` deletions
- direct overlap (`main...upstream-latest-release`): `599` files
- direct seam overlap candidates in upstream delta: `38` files

### High-level read (pre-alignment)

`0.124` changes a large amount of shared runtime surface without directly
rewriting the fork-owned policy crates. The highest-risk themes before ingest
are:

- protocol operation and event growth around turn environment selection,
  guardian warnings, and model verification
- session runtime changes across turn setup, context construction, handler
  dispatch, MCP/tool paths, and thread/task lifecycle boundaries
- config crate churn that may affect config schema and fork-owned runtime gates
- TUI slash-dispatch/chatwidget churn that can affect refresh/prune and fork
  command routing

### Seam table (ingest snapshot)

| File | Fork-owned bundles affected | Initial risk | Decision | Rationale | Validation |
| --- | --- | --- | --- | --- | --- |
| `codex-rs/protocol/src/{protocol.rs,models.rs,config_types.rs,request_permissions.rs}` | protocol carrier types consumed by maintenance, observability, and tool-surface adapters | very high | `accept_upstream` | Upstream protocol growth is additive for this ingest. Fork-owned artifact streams were updated to explicitly ignore `ResponseEvent::ModelVerifications` rather than weakening exhaustive matches. | `cargo check -p codex-core -p codex-app-server-client` |
| `codex-rs/core/src/session/{mod.rs,turn.rs,handlers.rs,session.rs,turn_context.rs,mcp.rs}` | strict-v1 prompt layering, context-maintenance runtime adapters, observability runtime adapters, E-witness tool/runtime wiring | very high | `merge_both` | Kept fork-owned `ProgressObservation` routing in session event sends while accepting upstream turn-environment selection and skill-warning changes. Dropped stale old core-local agent-identity registration helpers because 0.124 moves that surface into the new upstream crate. | `cargo check -p codex-core -p codex-app-server-client`; `just fmt` |
| `codex-rs/core/src/context_manager/updates.rs` | strict-v1 prompt layering and context-fragment update rendering | high | `accept_upstream` | The file auto-merged without fork-specific conflict in this ingest. It remains a watch point for follow-up behavioral validation if prompt-layer tests show drift. | `cargo check -p codex-core -p codex-app-server-client` |
| `codex-rs/core/src/{compact.rs,compact_remote.rs}` | continuation bridge, thread memory, compact artifact reinsertion, summary classification | high | `merge_both` | Remote compaction now keeps upstream rollout trace checkpoints while preserving fork-required thread-memory and continuation-bridge artifact generation/reinsertion policy. Fork artifact model calls were updated for upstream strict schema and trace parameters. | `cargo check -p codex-core -p codex-app-server-client`; `just fmt` |
| `codex-rs/core/src/tools/{orchestrator.rs,registry.rs,handlers/*}` | E-witness tool contract exposure, thread-spawn containment, unified exec behavior | high | `accept_upstream` | Tool surface files auto-merged. No fork-owned tool exposure change was needed for compile-level ingest, but targeted runtime tests still need to verify observability/tool containment after full validation. | `cargo check -p codex-core -p codex-app-server-client` |
| `codex-rs/config/src/{config_toml.rs,config_requirements.rs,hook_config.rs,lib.rs,types.rs}` | config gates for fork-owned features and schema expectations | medium-high | `merge_both` | Config code auto-merged except for config test imports. The workspace lock was updated with `cargo update -w --offline` so fork crates joined the 0.124 workspace without refreshing external dependency versions. | `cargo check -p codex-core -p codex-app-server-client` |
| `codex-rs/app-server-protocol/src/protocol/{common.rs,v2.rs}` and `codex-rs/app-server/src/codex_message_processor.rs` | app-server refresh/prune and thread operation surfaces | medium-high | `accept_upstream` | App-server protocol and processor files auto-merged. The remote app-server client shutdown conflict was resolved by preserving fork tolerance for a closed worker response channel while taking upstream's simplified shutdown flow. | `cargo check -p codex-core -p codex-app-server-client` |
| `codex-rs/tui/src/{chatwidget.rs,chatwidget/slash_dispatch.rs}` | slash command routing for fork maintenance commands | medium | `accept_upstream` | TUI slash/chatwidget changes auto-merged during raw ingest; targeted TUI command tests remain a post-merge validation item if alignment work touches this surface. | `cargo check -p codex-core -p codex-app-server-client` |

### Notes

- Local mirror refs were updated before ingest:
  - `upstream-latest-release` -> `rust-v0.124.0` / `e9fb49366c`
  - `upstream-main` -> `upstream/main` / `0db6811b7c`
- `rust-v0.125.0` already exists upstream. This ingest intentionally targets
  `rust-v0.124.0` first.
- Current ingest branch: `codex/update-0.124-ingest`.
- Merge conflicts resolved in `codex-rs/Cargo.toml`, `codex-rs/Cargo.lock`,
  `codex-rs/app-server-client/src/remote.rs`, `codex-rs/core/src/arc_monitor.rs`,
  `codex-rs/core/src/compact_remote.rs`, `codex-rs/core/src/config/config_tests.rs`,
  `codex-rs/core/src/session/mod.rs`, `codex-rs/core/src/session/turn_context.rs`,
  and `codex-rs/core/tests/suite/hooks.rs`.
- `cargo generate-lockfile` was avoided after it refreshed external dependency
  versions and exposed a `gix`/`winnow` mismatch; `cargo update -w --offline`
  produced the intended workspace-only lock update.
- Validation so far:
  - `cargo check -p codex-core -p codex-app-server-client`
  - `just write-config-schema`
  - `just write-app-server-schema`
  - `cargo test -p codex-app-server-protocol --test schema_fixtures`
  - `cargo test -p codex-config -p codex-tools`
  - `cargo test -p codex-core compact --lib`
  - `cargo test -p codex-core context_maintenance --lib`
  - `cargo test -p codex-core tools::spec::tests --lib`
  - `cargo test -p codex-app-server suite::v2::compaction::thread_refresh_start_triggers_compaction_and_returns_empty_response --test all -- --exact`
  - `cargo test -p codex-app-server suite::v2::compaction::thread_prune_start_triggers_compaction_and_returns_empty_response --test all -- --exact`
  - `just bazel-lock-update`
  - `just bazel-lock-check`
  - `just fmt`
- Alignment-only PR guard before any later PR to `main`:
  - `git merge-base --is-ancestor codex/update-0.124-ingest main`
  - `git log --oneline main..codex/update-0.124-align`

## 0.122.0 -> 0.123.0 (prep + ingest snapshot)

### Refs

- fork main at ingest start: `8a57fa8e7d`
- upstream target: `rust-v0.123.0` / `0785b66228` (tag object `d1005e4215`)
- live upstream inspection ref at prep time: `upstream-main` / `5e71da1424`
- comparison range for upstream release delta: `rust-v0.122.0..rust-v0.123.0`
- comparison range for fork ingest surface: `main..upstream-latest-release`

### Scale

- upstream release delta (`rust-v0.122.0..rust-v0.123.0`): `577` files changed, `44377` insertions, `19471` deletions
- current fork-main vs upstream target (`main..upstream-latest-release`): `724` files changed, `45349` insertions, `59097` deletions
- direct overlap (`main...upstream-latest-release`): `577` files
- direct seam overlap candidates in upstream delta: `15` files
  - `codex-rs/config/src/config_toml.rs`
  - `codex-rs/core/src/agent/control.rs`
  - `codex-rs/core/src/config/mod.rs`
  - `codex-rs/core/src/context_manager/updates.rs`
  - `codex-rs/core/src/session/agent_task_lifecycle.rs`
  - `codex-rs/core/src/session/handlers.rs`
  - `codex-rs/core/src/session/mcp.rs`
  - `codex-rs/core/src/session/mod.rs`
  - `codex-rs/core/src/session/session.rs`
  - `codex-rs/core/src/session/turn.rs`
  - `codex-rs/core/src/session/tests.rs`
  - `codex-rs/core/src/session/tests/guardian_tests.rs`
  - `codex-rs/protocol/src/models.rs`
  - `codex-rs/protocol/src/protocol.rs`
  - `codex-rs/tools/src/tool_registry_plan.rs`

### High-level read (pre-alignment)

`0.123` did not rewrite the fork-owned semantic crates directly (`context-maintenance-policy`,
`agent-observability`), but it did refactor multiple adapter seams those crates depend on:

- context/developer instruction rendering moved toward typed context fragments (`core::context::*`)
- protocol/tool surface grew around namespaced dynamic tools and realtime event variants
- config/runtime permission surfaces were reshaped (`FileSystemPermissions` canonical form, feature/config loader flow)

The dominant fork risk is adapter drift in `core/src/session/*` and
`core/src/context_manager/updates.rs`: if we accept upstream blindly there,
strict-v1 prompt layering and context-maintenance handoff assumptions can
silently change while still compiling.

### Seam table (initial ingest snapshot)

| File | Fork-owned bundles affected | Initial risk | Decision | Rationale | Validation |
| --- | --- | --- | --- | --- | --- |
| `codex-rs/core/src/session/{mod.rs,turn.rs,handlers.rs}` | strict-v1 prompt layering, context-maintenance runtime adapters, E-witness tool/runtime wiring | very high | `merge_both` | Upstream moved instruction/tool plumbing toward typed context fragments and namespaced dynamic tools; we keep fork-owned behavior and reattach through the new session seams. | `cargo test -p codex-core compact_tests --lib`; `cargo test -p codex-core config::schema::tests::config_schema_matches_fixture --lib` |
| `codex-rs/core/src/context_manager/updates.rs` | strict-v1 prompt layering updates | high | `merge_both` | Upstream replaced `DeveloperInstructions`-style updates with context fragment renderers; fork needs to preserve layered governance semantics on top of new render path. | `cargo test -p codex-core compact_tests --lib` |
| `codex-rs/tools/src/tool_registry_plan.rs` + `core/src/session/turn.rs` | E-witness tool contract exposure and planning interactions | high | `merge_both` | Dynamic tools now use namespaced loadable specs; fork keeps explicit tool-surface policy and waits/progress contract while adopting upstream namespaced filtering behavior. | `cargo test -p codex-core unified_exec::tests::unified_exec_persists_across_requests --lib`; `cargo test -p codex-core unified_exec::tests::multi_unified_exec_sessions --lib` |
| `codex-rs/protocol/src/{protocol.rs,models.rs}` | carrier types consumed by maintenance and observability adapters | high | `accept_upstream` | Protocol changes are mostly additive/refactor-level for our seams (`RealtimeNoopRequested`, namespaced dynamic tool response field, permissions shape migration). Keep upstream protocol as authority and adapt fork seams at call sites. | `cargo test -p codex-core compact_tests --lib`; `cargo test -p codex-core config::schema::tests::config_schema_matches_fixture --lib` |
| `codex-rs/core/src/config/mod.rs` + `codex-rs/config/src/config_toml.rs` | strict-v1 config gates, schema/versioned config expectations | medium-high | `merge_both` | Upstream added config/loader requirements and new feature surface (`semantic_broker` appeared in schema). Fork keeps governance-path config behavior while absorbing loader and schema updates. | `just write-config-schema`; `cargo test -p codex-core config::schema::tests::config_schema_matches_fixture --lib` |
| `codex-rs/core/src/agent/control.rs` | E-witness lifecycle adapter boundary | medium | `accept_upstream` | No direct semantic-owner drift in `agent-observability`; upstream lifecycle/auth changes can stay core-owned as long as reducer contracts remain intact. | `cargo test -p codex-core compact_tests --lib` |

### Notes (ingest snapshot)

- Local mirror refs were updated before ingest:
- `upstream-latest-release` -> `rust-v0.123.0` / `0785b66228`
  - `upstream-main` -> `upstream/main` / `5e71da1424`
- Pushing the read-only mirror branch back to `origin` remains blocked by repo
  policy for `upstream-latest-release`.
- Current ingest branch: `codex/update-0.123-ingest`.
- During validation, one expected schema fixture drift (`semantic_broker` in
  `core/config.schema.json`) was resolved by running `just write-config-schema`.
- A full `cargo test -p codex-core` pass hit two `unified_exec` test flakes in
  one run; both pass when re-run targeted:
  - `unified_exec::tests::unified_exec_persists_across_requests`
  - `unified_exec::tests::multi_unified_exec_sessions`

## 0.121.0 -> 0.122.0 (prep)

### Refs

- fork main at prep time: `b67200fbfa`
- upstream target: `rust-v0.122.0` / `230dcadee6` (tag object `9e1c5b0352`)
- live upstream inspection ref at prep time: `upstream-main` / `ca3246f77a`
- comparison range for upstream release delta: `rust-v0.121.0..rust-v0.122.0`
- comparison range for fork ingest surface: `main..upstream-latest-release`

### Scale

- upstream release delta (`rust-v0.121.0..rust-v0.122.0`): `778` files changed, `62857` insertions, `18347` deletions
- current fork-main vs upstream target (`main..upstream-latest-release`): `892` files changed, `63507` insertions, `47791` deletions
- direct overlap (`main...upstream-latest-release`): `778` files
- known fork seam overlap candidates present in upstream delta: `15` files
  - `codex-rs/protocol/src/protocol.rs`
  - `codex-rs/protocol/src/models.rs`
  - `codex-rs/core/src/agent/control.rs`
  - `codex-rs/core/src/compact.rs`
  - `codex-rs/core/src/compact_remote.rs`
  - `codex-rs/core/src/context_manager/history.rs`
  - `codex-rs/core/src/context_manager/updates.rs`
  - `codex-rs/core/src/event_mapping.rs`
  - `codex-rs/core/src/thread_manager.rs`
  - `codex-rs/core/src/tools/spec.rs`
  - `codex-rs/core/src/tools/router.rs`
  - `codex-rs/tools/src/tool_registry_plan.rs`
  - `codex-rs/config/src/config_toml.rs`
  - `codex-rs/app-server-protocol/src/protocol/v2.rs`
  - `codex-rs/app-server/src/codex_message_processor.rs`

### High-level read (pre-alignment)

`0.122` is not primarily a compaction-law rewrite. The main challenge is
runtime ownership movement around the seams we already extracted.

Highest-risk themes before ingest:

- upstream deleted `core/src/codex.rs` and moved large runtime ownership into
  new `core/src/session/*` modules
- observability now has a new protocol event to classify:
  `EventMsg::PatchApplyUpdated`
- session-source handling changed with
  `SubAgentSource::MemoryConsolidation`, which can affect
  `AgentToolSurfacePolicy`
- tool planning changed around deferred dynamic tools and unavailable-tool
  placeholders, so our A3/A4 observability alignment points need review
- context-maintenance executor files changed only lightly, but protocol and
  runtime carrier types changed enough that source-selection, rollout, and
  replacement-history assumptions need another pass

### Seam table (initial prep)

| File | Fork-owned bundles affected | Initial risk | Decision | Rationale | Validation |
| --- | --- | --- | --- | --- | --- |
| `codex-rs/core/src/{codex.rs -> session/*}` | context-maintenance runtime adapters, observability runtime adapters | very high | `merge_both` | Upstream runtime ownership moved into `session/*`; we kept module-owned law in the extracted crates and reattached the fork through `session::turn`, `session::turn_context`, `session::mod`, and `session::handlers`. The critical correction was restoring `RefreshContext` / `PruneContext` dispatch in `session/handlers.rs` after upstream dropped those ops during the split. | `cargo check -p codex-core`; `cargo test -p codex-core context_maintenance --lib`; `cargo test -p codex-tui slash_refresh_dispatches_refresh_context_op --lib`; `cargo test -p codex-tui slash_prune_dispatches_prune_context_op --lib` |
| `codex-rs/protocol/src/protocol.rs` | observability event classification, session-source/tool-surface policy, rollout carrier types | high | `defer` | New protocol surface includes `EventMsg::PatchApplyUpdated`, `SubAgentSource::MemoryConsolidation`, `RolloutItem::SessionState`, and extra turn-context fields; these can compile while silently changing our adapter assumptions. | pre-ingest only |
| `codex-rs/protocol/src/models.rs` | context-maintenance source selection, history shaping, protocol utility expectations | high | `defer` | `ContentItem::InputImage` and `GhostCommit` moved or changed in ways that may affect source filtering, image-detail handling, and rollout-related helpers. | pre-ingest only |
| `codex-rs/core/src/compact.rs` | local compact assembly, artifact reinsertion, timing/injection adapters | medium-high | `merge_both` | Kept the fork-owned compaction engine routing, timing classification, authoritative artifact generation, and deterministic local replacement-history assembly while absorbing the new session host paths. Also revalidated the local compact invariants after the ingest to make sure stale bridge/thread-memory items still drop by construction. | `cargo test -p codex-core compact::tests --lib`; `cargo test -p codex-core context_maintenance --lib` |
| `codex-rs/core/src/compact_remote.rs` | remote compact shaping, summary classification, artifact reinsertion | medium-high | `merge_both` | Remote compact itself changed lightly in `0.122`; the ingest preserved the fork's policy-owned shaping and classifier adapters while switching to the new session runtime entry points. | `cargo check -p codex-core`; `cargo test -p codex-core compact::tests --lib` |
| `codex-rs/core/src/context_manager/{history.rs,updates.rs}` | prompt/history visibility and final-history validity for maintenance artifacts | medium-high | `defer` | Source-selection and replacement-history assumptions depend on what prompt/history APIs expose and normalize. Small upstream changes here can bypass policy expectations without obvious compile failures. | pre-ingest only |
| `codex-rs/core/src/event_mapping.rs` | context-maintenance classifiers for user/contextual/summary items | medium-high | `defer` | Fork history-shape decisions still depend on core-supplied classification logic; this remains a review point even though the raw diff is small. | pre-ingest only |
| `codex-rs/core/src/{agent/control.rs,thread_manager.rs}` | observability registry lifetime, progress seeding/inspection/removal | high | `defer` | Upstream control-plane and thread-manager changes must still seed, update, inspect, and remove progress state correctly after the new session refactor. | pre-ingest only |
| `codex-rs/core/src/tools/{spec.rs,router.rs}` and `codex-rs/tools/src/tool_registry_plan.rs` | E-witness tool contract wiring, tool-surface containment, unavailable/deferred tool exposure | high | `defer` | Upstream introduced new tool-planning behavior around deferred dynamic tools and unavailable tool placeholders; our observability contract and explicit tool-surface policy need to be realigned there. | pre-ingest only |
| `codex-rs/config/src/config_toml.rs` | context-maintenance adapter translation from config | medium | `defer` | Config churn appears secondary to the runtime split, but the policy crate must remain config-decoupled and the adapter translation still needs review. | pre-ingest only |
| `codex-rs/app-server-protocol/src/protocol/v2.rs` and `codex-rs/app-server/src/codex_message_processor.rs` | refresh/prune/thread maintenance protocol surfaces | medium | `merge_both` | The `thread/refresh/start` and `thread/prune/start` protocol surfaces survived the release and still submit the fork ops through app-server. No extra fork-specific app-server rewrite was needed once the core/session dispatcher was restored. | `cargo test -p codex-app-server suite::v2::compaction::thread_refresh_start_triggers_compaction_and_returns_empty_response --test all -- --exact`; `cargo test -p codex-app-server suite::v2::compaction::thread_prune_start_triggers_compaction_and_returns_empty_response --test all -- --exact` |

### Notes (prep)

- Local mirror refs were updated before prep:
  - `upstream-latest-release` -> `rust-v0.122.0` / `230dcadee6`
  - `upstream-main` -> `upstream/main` / `ca3246f77a`
- Pushing the read-only mirror branches back to `origin` is blocked by repo
  policy, so the mirror refresh is currently local-only.
- Immediate manual review questions for this ingest:
  - how should `EventMsg::PatchApplyUpdated` classify in
    `codex-agent-observability`?
  - what tool-surface policy should apply to
    `SubAgentSource::MemoryConsolidation`?
  - where did the old `codex.rs`-level context-maintenance hooks move inside
    `core/src/session/*`?
- Start from fork `main`; do not build the ingest branch from
  `upstream-latest-release`.

### Alignment progress (in flight)

- Resolved the main `codex.rs` -> `session/*` migration by reattaching fork
  adapters instead of reopening module law:
  - `core/src/session/turn.rs` now passes explicit
    `CompactInvocationTiming` for intra-turn vs turn-boundary auto-compact
  - `core/src/session/turn_context.rs` regained the fork adapter helpers for
    continuation-bridge and governance config lookup
  - `core/src/session/mod.rs` now rebuilds the settings-update overlay with
    the fork-owned governance/prompt sections on top of the new upstream
    settings-diff API
  - `core/src/session/handlers.rs` again dispatches `RefreshContext` and
    `PruneContext` through `ContextMaintenanceTask`
- TUI/app-side fallout from the session split was small but real:
  - `/refresh` and `/prune` slash commands needed to be restored in queued
    drain behavior
  - the stale `DEFAULT_PROJECT_DOC_FILENAME` import was removed after
    upstream fully converged on `AGENTS.md`
- New upstream stream/protocol surface already absorbed during this pass:
  - `ResponseEvent::ToolCallInputDelta` is now explicitly ignored in the
    continuation-bridge and thread-memory stream readers
  - `ContentItem::InputImage` test fixtures now include the new `detail`
    field required by `0.122`

### Validation (current)

- `cargo check -p codex-core`
- `cargo test -p codex-context-maintenance-policy`
- `cargo test -p codex-agent-observability`
- `cargo test -p codex-core compact::tests --lib`
- `cargo test -p codex-core context_maintenance --lib`
- `cargo test -p codex-core tools::handlers::agent_progress::tests --lib`
- `cargo test -p codex-tools agent_progress_tool --lib`
- `cargo test -p codex-tui slash_refresh_dispatches_refresh_context_op --lib`
- `cargo test -p codex-tui slash_prune_dispatches_prune_context_op --lib`

### Remaining watchpoints

- broader protocol churn in `protocol.rs` / `models.rs` still needs a final
  sweep before the ingest can be called complete, but the concrete
  `PatchApplyUpdated`, `ToolCallInputDelta`, `MemoryConsolidation`, and
  `InputImage.detail` deltas are already aligned
- upstream `tui/src/chatwidget/slash_dispatch.rs` in `0.122` now has a queued
  slash-command edge case for commands without inline args: if queued input
  includes trailing text while a task is already running, the queued path can
  submit the raw command text to the model instead of dispatching the slash
  command. This is not fork-specific, but it is relevant to our restored
  `/refresh` and `/prune` flows because they use the same dispatcher. Treat it
  as an upstream behavior watchpoint on later updates unless it starts blocking
  fork workflows directly.

### Post-prep follow-up decisions already landed on the ingest branch

- `EventMsg::PatchApplyUpdated` is now explicitly reduced by
  `codex-agent-observability` as patch-tool progress:
  - the first update promotes tool-call progress and bumps `seq`
  - subsequent updates stay heartbeat-only while still refreshing the active
    work label as the patch grows
- `SubAgentSource::MemoryConsolidation` now has an explicit
  `AgentToolSurfacePolicy`:
  - no collaboration surface
  - no `spawn_agent`
  - no `request_user_input`
  This keeps the internal memory-consolidation agent from inheriting the broad
  generic subagent tool surface by accident.
- deferred/unavailable tool planning was rechecked against the A3/A4
  observability alignment:
  - code-mode-only still hides the nested tool surface correctly
  - deferred dynamic tool search still registers and describes the deferred
    tools correctly
  - no additional observability-specific rewrite was needed beyond the current
    router/tool-surface alignment
- app-server refresh/prune surfaces were revalidated end-to-end through the
  `tests/all.rs` compaction suite, so the remaining ingest work is no longer in
  the thread-maintenance transport layer

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

### Seam table (alignment decisions)

| File | Fork-owned bundles affected | Initial risk | Decision | Rationale | Validation |
| --- | --- | --- | --- | --- | --- |
| `codex-rs/core/src/config/mod.rs` | continuation bridge config, governance mode/profile config | high | `merge_both` | Accepted upstream config host evolution while keeping fork keys and defaults intact. | merged cleanly; covered by focused `codex-core` compile/tests |
| `codex-rs/core/config.schema.json` | strict/governance and continuation config schema | high | `merge_both` | Preserved fork schema surfaces while ingesting upstream schema evolution. | merged cleanly; no schema regeneration needed in this turn |
| `codex-rs/core/src/tools/spec.rs` | E-witness tools, thread-spawn tool containment | high | `merge_both` | Retained fork tool visibility/containment semantics on top of upstream tool-surface updates. | merged cleanly; validated by `tools::spec::tests::` |
| `codex-rs/core/src/agent/control.rs` | E-witness lifecycle integration, continuation bridge sub-agent carryover | high | `merge_both` | Kept fork progress/lifecycle behavior while absorbing upstream control-plane changes. | merged cleanly; compile passed in focused run |
| `codex-rs/core/src/codex.rs` | compaction injection points, governance prompt layering hooks | high | `merge_both` | Preserved fork compaction/governance insertion seams; fixed one async plugin-loading drift after merge. | follow-up fix committed in `2afd7d4aae`; focused tests pass |
| `codex-rs/core/src/compact.rs` | local compaction wrappers, bridge/thread-memory insertion, fail-closed behavior | high | `merge_both` | Kept local-compaction fork wrappers and fail-closed semantics while ingesting upstream host changes. | merged cleanly; compile passed in focused run |
| `codex-rs/core/src/compact_remote.rs` | remote compaction wrapping and post-compact artifact insertion | high | `merge_both` | Preserved post-`/responses/compact` artifact reinsertion semantics over upstream remote changes. | merged cleanly; compile passed in focused run |
| `codex-rs/core/src/context_manager/updates.rs` | governance prompt-layer settings update propagation | medium | `merge_both` | Maintained governance prompt-layer propagation while accepting upstream update mechanics. | merged cleanly; compile passed in focused run |
| `codex-rs/core/src/thread_manager.rs` | E-witness continuity during fork/resume/history reconstruction | medium | `merge_both` | Kept fork continuity expectations while ingesting upstream thread-manager updates. | merged cleanly; compile passed in focused run |
| `codex-rs/core/src/codex_tests.rs` | seam-level behavior assertions | medium | `merge_both` | Accepted upstream test-host changes and retained fork seam expectations. | compile passed; full-suite deferred to final validation checkpoint |
| `codex-rs/core/src/tools/spec_tests.rs` | E-witness + containment test coverage | medium | `merge_both` | Resolved conflict by preserving both upstream namespace assertions and fork absence assertions. | `tools::spec::tests::` passed |
| `codex-rs/core/tests/suite/compact.rs` | local compaction behavior coverage | medium | `merge_both` | Carried fork compaction expectations on top of upstream suite updates. | merged cleanly; full suite deferred |
| `codex-rs/core/tests/suite/compact_remote.rs` | remote compaction behavior coverage | medium | `merge_both` | Carried fork remote-compaction expectations on top of upstream suite updates. | merged cleanly; full suite deferred |
| `codex-rs/core/tests/suite/compact_resume_fork.rs` | compact/fork/resume interplay | low-medium | `merge_both` | Preserved fork continuity/fork-resume expectations while ingesting upstream updates. | merged cleanly; full suite deferred |

### Notes (prep)

- `origin/upstream-main` has been force-synced to `rust-v0.121.0` (`d65ed92a5e`) as the canonical mirror.
- Ingest branch was created from fork canonical line: `codex/update-0.121`.
- Alignment was applied with `merge_both` as the default across all tracked seam candidates.
- Merge conflicts were limited to:
  - `codex-rs/Cargo.toml`
  - `codex-rs/core/src/tools/handlers/multi_agents_tests.rs`
  - `codex-rs/core/src/tools/spec_tests.rs`
- Follow-up compile-drift fixes after merge:
  - async await for `plugins_for_config()` in `codex.rs`
  - absolute-path test updates in `agent/progress.rs` and `config/config_tests.rs`
  - lockfile workspace-version refresh to `0.121.0`
- Fast sanity checks run in this checkpoint:
  - `cd codex-rs && just fmt`
  - `cd codex-rs && cargo test -p codex-core tools::spec::tests::`
  - `cd codex-rs && cargo test -p codex-core handler_rejects_non_function_payloads`

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

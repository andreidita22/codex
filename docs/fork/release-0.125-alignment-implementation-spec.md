# Release 0.125 Post-Ingest Alignment Implementation Spec

This spec defines the fork-owned alignment work to perform after ingesting
upstream `rust-v0.125.0`.

Primary references:

- [release-alignment-log.md](release-alignment-log.md)
- [custom-fork-module-inventory.md](custom-fork-module-inventory.md)
- [fork-update-checklist.md](fork-update-checklist.md)

## Current State

`rust-v0.125.0` has already been ingested into fork `main` using the Path B
topology from the update checklist:

- ingest branch: `codex/update-0.125-ingest`
- ingest commit: `43a8773294 merge: ingest upstream rust-v0.125.0`
- alignment branch: `codex/update-0.125-align`
- alignment branch delta at spec time: empty

The alignment-only PR guard passed at spec time:

```sh
git merge-base --is-ancestor codex/update-0.125-ingest main
git log --oneline main..codex/update-0.125-align
```

The second command returned no commits. Any PR opened from
`codex/update-0.125-align` to `main` should therefore contain only fork-owned
alignment commits, not upstream release commits.

## Purpose

The 0.125 ingest preserved compile-time compatibility and the most important
fork seams. This alignment pass should now prove the preserved fork behavior is
still semantically correct on top of upstream 0.125.

The goal is not to re-ingest upstream, rework the merge, or broaden the fork.
The goal is to add focused tests and small corrections where upstream 0.125
changed shared runtime paths enough that compile checks are not sufficient.

## In Scope

1. Add targeted integration coverage for strict-v1 prompt layering after PR #45
   and the 0.125 session/environment refactors.
2. Add or run targeted coverage for continuation bridge and thread-memory
   behavior on local and remote compaction paths.
3. Verify E-witness progress tools still observe live sub-agent progress after
   upstream rollout-trace and live-thread changes.
4. Verify thread-spawn sub-agent containment still hides `spawn_agent` while
   preserving observability tools.
5. Record any discovered alignment decisions in the release-alignment log.

## Out Of Scope

This alignment pass should not:

- ingest `rust-v0.126.0-alpha.1`
- refactor the raw 0.125 merge
- reopen broad compaction-policy architecture
- move fork-owned crates or modules unless a failing alignment check requires it
- change app-server refresh/prune API shape unless tests expose a regression
- change TUI rendering unless a targeted alignment check exposes drift

## Required Invariants

### Review Topology

- `main` remains the canonical fork baseline.
- `codex/update-0.125-align` starts from the 0.125-ingested `main`.
- Alignment PRs target `main` only after the guard above still passes.
- Alignment commits must be fork-owned and reviewable without upstream release
  commits in the PR diff.

### Runtime Semantics

- Strict-v1 prompt layering remains opt-in and governed by
  `governance_path_variant`.
- Initial context and settings-update paths use the same layer ordering:
  constitutional, role, task, runtime.
- Continuation bridge and thread memory artifacts remain generated and
  reinserted according to context-maintenance policy.
- Required thread-memory generation remains fail-closed in strict paths.
- E-witness progress events remain observable from parent sessions.
- Thread-spawned sub-agents keep observability tools but do not receive
  `spawn_agent`.
- App-server `thread/refresh/start` and `thread/prune/start` remain present in
  Rust protocol types and generated JSON/TypeScript schemas.

## Work Packages

### A1: Prompt-Layering Coverage After 0.125 Session Refactors

Problem:

- PR #45 centralized initial-context section assembly.
- Upstream 0.125 changed session construction, turn environment selection, and
  settings-update behavior.
- Existing compile checks do not prove that initial context and update context
  still preserve strict-v1 prompt-layer insertion and ordering.

Target design:

- Add focused tests around the prompt-visible context generated at thread start.
- Add focused tests around settings/context updates after a turn configuration
  change.
- Prefer existing core test harnesses and response-request inspection helpers.

Required checks:

1. `strict_v1_shadow` emits one `<governance_prompt_layers ...>` block at
   initial thread start.
2. The block contains expected constitutional, role, task, and runtime layer
   categories when corresponding sections exist.
3. A settings update that changes runtime/developer context still emits or
   preserves the layering block as intended.
4. `governance_path_variant = off` does not emit the block.

Likely touched files:

- `codex-rs/core/src/context_manager/updates.rs`
- `codex-rs/core/src/session/mod.rs`
- `codex-rs/core/src/session/tests.rs`
- `codex-rs/core/tests/suite/model_visible_layout.rs` or another existing
  prompt-shape integration suite

Validation:

```sh
cd codex-rs
just fmt
cargo test -p codex-core governance::prompt_layers --lib
cargo test -p codex-core model_visible --test all
```

If the exact integration filter differs, run the concrete test names added in
this work package.

Status:

- Completed in `codex-rs/core/tests/suite/model_visible_layout.rs`.
- Added request-level coverage for strict-v1 initial context, strict-v1
  settings-update context, and governance-off omission.
- The second-turn request intentionally contains both the original initial
  prompt-layer block from history and the new `settings_update` block; the test
  asserts exactly one block for each phase.
- While compiling the focused integration target, one stale 0.125 test
  initializer in `codex-rs/core/tests/suite/compact_remote.rs` needed
  `environments: None` for `Op::UserInput`.

Validation run:

```sh
cd codex-rs
just fmt
cargo test -p codex-core governance --test all
cargo test -p codex-core governance::prompt_layers --lib
```

### A2: Continuation Bridge And Thread-Memory Remote Path Verification

Problem:

- The ingest passed local compaction and context-maintenance filters.
- Upstream 0.125 replaced more rollout persistence with live-thread storage and
  expanded rollout trace.
- Remote compaction is the highest-risk path for bridge and thread-memory
  reinsertion because it crosses model calls, rollout reconstruction, and
  persisted thread state.

Target design:

- Run existing remote compaction tests first.
- Add focused tests only if current coverage does not explicitly prove the
  0.125 seam.

Required checks:

1. Remote compaction still generates continuation bridge requests when
   configured.
2. Remote compaction still reinserts `<continuation_bridge ...>` into
   replacement history.
3. Strict governed compaction still generates or reuses `<thread_memory ...>`.
4. Required thread-memory failure still aborts and emits the local error event.
5. Live-thread rollback/reconstruction still preserves compacted artifact
   invariants after upstream persistence changes.

Likely touched files:

- `codex-rs/core/src/compact_remote.rs`
- `codex-rs/core/src/compact.rs`
- `codex-rs/core/src/governance/thread_memory.rs`
- `codex-rs/core/src/continuation_bridge.rs`
- `codex-rs/core/tests/suite/compact_remote.rs`
- `codex-rs/core/tests/suite/compact.rs`

Validation:

```sh
cd codex-rs
just fmt
cargo test -p codex-core compact_remote --test all
cargo test -p codex-core compact --test all
cargo test -p codex-core compact --lib
```

Status:

- Completed focused remote-path coverage in
  `codex-rs/core/tests/suite/compact_remote.rs`.
- Added a thread-memory model-call responder in
  `codex-rs/core/tests/common/responses.rs`.
- Added strict remote-hybrid manual compaction coverage proving that:
  - required thread memory is generated before the compact endpoint call,
  - the thread-memory request sees the source user and assistant history,
  - generated `<thread_memory ...>` is reinserted into the follow-up request,
  - the remote compact summary remains present in replacement history, and
  - turn-boundary remote compaction does not issue a continuation bridge.
- Added a fail-closed strict path proving invalid required thread-memory output
  stops before `/v1/responses/compact`.
- Test responder request logs are schema-filtered before assertions because the
  shared `ResponseMock` recorder observes all `/v1/responses` requests evaluated
  by a mounted mock.

Validation run:

```sh
cd codex-rs
just fmt
cargo test -p codex-core remote_compact_with_strict_governance_reinserts_thread_memory --test all
cargo test -p codex-core strict_remote_compact_thread_memory_failure_skips_compact_endpoint --test all
cargo test -p codex-core remote_compact_does_not_issue_bridge_request_when_override_is_configured --test all
cargo test -p codex-core compaction_policy_matrix --lib
cargo test -p codex-context-maintenance-policy thread_memory
```

### A3: E-Witness Progress And Rollout-Trace Coexistence

Problem:

- The ingest preserved `ProgressObservation` and added upstream
  `rollout_thread_trace.record_protocol_event(...)` into the same event path.
- Compile checks prove the APIs compose, but not that progress observation still
  records the right live sub-agent phases.

Target design:

- Add or run integration tests that exercise a spawned child session and inspect
  progress through the fork-owned E-witness tools.
- Confirm compatibility-only events do not incorrectly advance observed
  progress.

Required checks:

1. `inspect_agent_progress` returns a coherent snapshot after child spawn.
2. `wait_for_agent_progress` can observe a material phase transition.
3. Child completion still records observable progress for the parent.
4. Rollout-trace protocol recording does not duplicate or suppress E-witness
   progress events.

Likely touched files:

- `codex-rs/core/src/agent/control.rs`
- `codex-rs/core/src/session/mod.rs`
- `codex-rs/core/src/tools/handlers/agent_progress.rs`
- `codex-rs/core/tests/suite/subagent_notifications.rs`
- `codex-rs/tools/src/agent_progress_tool.rs`

Validation:

```sh
cd codex-rs
just fmt
cargo test -p codex-tools agent_progress_tool
cargo test -p codex-core tools::handlers::agent_progress --lib
cargo test -p codex-core subagent_notifications --test all
```

Status:

- Completed focused coexistence coverage in
  `codex-rs/core/src/tools/handlers/multi_agents_tests.rs`.
- Added a test that enables a real rollout trace bundle, sends a
  `ProgressObservation::CompatibilityOnly` protocol event through
  `send_event_raw`, and proves:
  - the compatibility-only event is recorded as a rollout trace protocol event,
  - the E-witness progress sequence does not advance, and
  - the progress snapshot remains at `seq = 0`.
- Re-ran the existing live progress handler checks for inspect/wait behavior,
  raw observed events, legacy echo deduplication, and public progress tool
  schemas.

Validation run:

```sh
cd codex-rs
just fmt
cargo test -p codex-core compatibility_only_protocol_events_are_traced_without_progress_observation --lib
cargo test -p codex-core inspect_agent_progress_reports_reasoning_phase_for_live_subagent --lib
cargo test -p codex-core wait_for_agent_progress --lib
cargo test -p codex-core session_send_event_observes_primary_item_started_once_with_legacy_echoes --lib
cargo test -p codex-core observed_raw --lib
cargo test -p codex-tools agent_progress_tool
```

### A4: Thread-Spawn Tool Containment Recheck

Problem:

- Upstream 0.125 changed tool/runtime routing around MCP, shell, unified exec,
  and plugin movement.
- The fork invariant is subtle: thread-spawned sub-agents should retain
  observability tools while not receiving `spawn_agent`.

Target design:

- Add a focused test at the tool-plan layer if existing tests do not assert the
  exact 0.125 behavior.
- Prefer `codex-tools` tests for the policy shape and `codex-core` tests only
  if runtime source mapping is suspect.

Required checks:

1. Root/default sessions expose `spawn_agent` when collaboration tools are
   enabled.
2. Thread-spawned sub-agents do not expose `spawn_agent`.
3. Thread-spawned sub-agents do expose `inspect_agent_progress` and
   `wait_for_agent_progress`.
4. Memory-consolidation sub-agents still receive the stricter hidden tool
   surface intended for that source.

Likely touched files:

- `codex-rs/tools/src/tool_config.rs`
- `codex-rs/tools/src/tool_registry_plan_tests.rs`
- `codex-rs/core/src/tools/spec.rs`
- `codex-rs/core/src/tools/spec_tests.rs`

Validation:

```sh
cd codex-rs
just fmt
cargo test -p codex-tools thread_spawn_subagents_keep_observability_but_hide_spawn
cargo test -p codex-core tools::spec::tests --lib
```

Status:

- Completed final tool-registry coverage in
  `codex-rs/tools/src/tool_registry_plan_tests.rs`.
- Added a thread-spawn subagent registry-plan test proving that the final
  model-visible tool list and handler registry:
  - keep `inspect_agent_progress` and `wait_for_agent_progress`,
  - keep non-spawning coordination tools (`send_message`, `followup_task`,
    `wait_agent`, `close_agent`, `list_agents`),
  - omit `spawn_agent`, `spawn_agents_on_csv`, `resume_agent`, and
    `request_user_input`, and
  - do not register a `spawn_agent` handler for thread-spawn subagents.
- Re-ran adjacent tool-surface tests for memory-consolidation subagents and
  agent-job subagents so the three subagent classes stay distinguished.

Validation run:

```sh
cd codex-rs
just fmt
cargo test -p codex-tools thread_spawn_subagents --lib
cargo test -p codex-tools memory_consolidation_subagents_do_not_receive_collaboration_tools --lib
cargo test -p codex-tools test_build_specs_agent_job_worker_tools_enabled --lib
cargo test -p codex-tools subagents_keep_request_user_input_mode_config_and_agent_jobs_workers_opt_in_by_label --lib
```

### A5: Alignment Log And Watchlist Closure

Problem:

- The raw ingest log records conflict decisions and validation.
- The post-ingest alignment branch should record any semantic findings,
  especially if a work package discovers that no code change is needed.

Target design:

- Add a short post-ingest alignment note to `release-alignment-log.md` once
  work packages are complete.
- Update `custom-fork-module-inventory.md` only if this pass changes the fork
  surface area or adds a new hot file.

Required checks:

1. Record which work packages were implemented or verified as no-op.
2. Record validation commands and any intentionally deferred checks.
3. Re-run the alignment-only PR guard before opening the PR.

Likely touched files:

- `docs/fork/release-alignment-log.md`
- `docs/fork/custom-fork-module-inventory.md` only if the inventory changes

Validation:

```sh
git merge-base --is-ancestor codex/update-0.125-ingest main
git log --oneline main..codex/update-0.125-align
```

The second command must list only fork-owned alignment commits.

Status:

- Completed the post-ingest closure note in
  `docs/fork/release-alignment-log.md`.
- Left `docs/fork/custom-fork-module-inventory.md` unchanged because A1-A4 add
  focused verification for existing fork-owned runtime bundles and do not add,
  remove, or replace a bundle.
- Re-ran the alignment-only PR guard on 2026-04-25. At that point
  `codex/update-0.125-ingest` was an ancestor of `main`, and
  `git log --oneline main..codex/update-0.125-align` produced no output because
  the A1-A5 work was still uncommitted.

## Recommended PR Sequence

Keep the first PR small and verification-heavy:

1. `125-A1`: prompt-layering tests and any minimal fix required by those tests.
2. `125-A2`: remote compaction/thread-memory verification and any minimal fix.
3. `125-A3/A4`: E-witness progress plus thread-spawn containment checks.
4. `125-A5`: final release-log closure if not already included.

If A1 produces only tests and no runtime changes, it can include A5 log updates
for the prompt-layering result. Avoid combining remote compaction fixes with
tool-surface fixes unless both are purely test-only.

## Validation Plan

Minimum validation before opening the first alignment PR:

```sh
cd codex-rs
just fmt
cargo test -p codex-core governance::prompt_layers --lib
cargo test -p codex-core context_maintenance --lib
cargo test -p codex-core tools::spec::tests --lib
cargo test -p codex-tools
```

Additional validation before merging all 0.125 alignment work:

```sh
cd codex-rs
cargo test -p codex-core compact_remote --test all
cargo test -p codex-core compact --test all
cargo test -p codex-core subagent_notifications --test all
cargo test -p codex-app-server suite::v2::compaction::thread_refresh_start_triggers_compaction_and_returns_empty_response --test all -- --exact
cargo test -p codex-app-server suite::v2::compaction::thread_prune_start_triggers_compaction_and_returns_empty_response --test all -- --exact
```

If any work package touches config schema or app-server protocol types, also
run:

```sh
cd codex-rs
just write-config-schema
just write-app-server-schema
cargo test -p codex-app-server-protocol --test schema_fixtures
```

## Deferred Follow-Ups

These are intentionally not part of the first 0.125 alignment PR unless a test
failure makes them necessary:

- broader TUI snapshot refresh
- full workspace `cargo test`
- alpha-release (`0.126.0-alpha.1`) ingest preparation
- policy-crate architecture changes from older compaction follow-up specs
- broad cleanup of legacy release-rebase scripts

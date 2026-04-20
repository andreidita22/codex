# Agent Observability Extraction Plan

This plan takes the accepted E-witness findings from [agent-observability-3rd-audit-review.md](/home/rose/work/codex/fork/docs/agent-observability-3rd-audit-review.md) and turns them into a concrete extraction program.

This is the next extraction family after context-maintenance.

## Scope

In scope:

- E-witness live sub-agent progress inspection and wait semantics
- progress reducer and registry ownership
- model-visible progress tool contract
- explicit thread-spawn tool-surface policy in `codex-tools`

Out of scope:

- cloud / `codex cloud` task surface
- broad `AgentControl` refactors
- generic multi-agent architecture redesign
- thread lifecycle orchestration extraction
- TUI/app-server work
- generic fork hub design

## Goal

Move fork-owned agent observability law out of `codex-core` and into a dedicated crate, while keeping runtime orchestration, target resolution, and tool registration wiring in their current crates.

The desired end state is:

- `codex-agent-observability` owns progress semantics
- `codex-tools` owns explicit collaboration tool-surface policy
- `codex-core` acts as adapter/runtime wiring only

## Real Semantic Boundary

The semantic owner should control:

- `AgentProgressPhase`
- blocker and active-work classification
- progress snapshots
- event-to-progress reduction
- material-progress sequence advancement
- stall calculation
- wait outcome/match classification
- semantic interpretation of protocol-owned lifecycle status
- canonical progress-tool names
- canonical progress-phase string list used by tool schemas
- canonical blocker-reason, active-work-kind, and wait-outcome string sets used by tool schemas

The semantic owner should not control:

- resolving tool arguments to target agents
- session/runtime waiting loops
- thread lifecycle seeding/removal hooks
- tool registry handler mapping
- thread-spawn session-source interpretation

The adapter/runtime side should still own:

- retrieving lifecycle status from live thread/runtime state

The semantic owner should decide how protocol-owned lifecycle status affects:

- finality
- phase overrides
- snapshot output

## Current Touch Points

Primary fork-owned semantic file:

- [progress.rs](/home/rose/work/codex/fork/codex-rs/core/src/agent/progress.rs)

Primary runtime adapters that should remain adapter-side:

- [control.rs](/home/rose/work/codex/fork/codex-rs/core/src/agent/control.rs)
- [thread_manager.rs](/home/rose/work/codex/fork/codex-rs/core/src/thread_manager.rs)
- [codex.rs](/home/rose/work/codex/fork/codex-rs/core/src/codex.rs)
- [agent_progress.rs](/home/rose/work/codex/fork/codex-rs/core/src/tools/handlers/agent_progress.rs)
- [spec.rs](/home/rose/work/codex/fork/codex-rs/core/src/tools/spec.rs)

Primary tool-contract surfaces:

- [agent_progress_tool.rs](/home/rose/work/codex/fork/codex-rs/tools/src/agent_progress_tool.rs)
- [tool_registry_plan.rs](/home/rose/work/codex/fork/codex-rs/tools/src/tool_registry_plan.rs)
- [tool_config.rs](/home/rose/work/codex/fork/codex-rs/tools/src/tool_config.rs)
- [tool_registry_plan_types.rs](/home/rose/work/codex/fork/codex-rs/tools/src/tool_registry_plan_types.rs)
- [router.rs](/home/rose/work/codex/fork/codex-rs/core/src/tools/router.rs)

## Desired Crate Shape

New workspace crate:

- `codex-rs/agent-observability`

Cargo package:

- `codex-agent-observability`

Expected direct dependencies:

- `codex-protocol`
- `serde`
- `tokio`
- optionally `tracing`

Required workspace wiring once implementation starts:

- add `"agent-observability"` to workspace members
- add `codex-agent-observability = { path = "agent-observability" }` to workspace dependencies
- add dependency edges from `codex-core` and later `codex-tools`
- do not introduce any dependency edge back from `codex-agent-observability` to `codex-core` or `codex-tools`

Dependencies it should not take:

- `codex-core`
- `codex-tools`
- `codex-config`
- CLI/TUI/app-server crates

Likely owned modules:

- `progress.rs`
- `wait.rs`
- `tool_contract.rs`

## Target Runtime Shape

After extraction:

- `AgentControl` translates live thread events and lifecycle state into calls on the observability crate
- `ThreadManager` stores a registry type owned by the observability crate
- progress tool handlers consume observability-owned snapshots and wait semantics
- `codex-tools` consumes observability-owned tool-name and schema-driving enum/string constants
- thread-spawn containment becomes an explicit `codex-tools` policy object

## PR Sequence

### PR A0 — Behavior locks

Purpose:

- freeze current progress semantics and containment behavior before ownership moves

Lock:

- this PR locks current behavior only; it must not pretend the final semantic owner already exists
- phase schema matches runtime phase list
- thread-spawned sub-agents lose `spawn_agent`
- sub-agents lose `request_user_input`
- thread-spawned sub-agents retain progress observability tools
- sequence advancement only occurs on material progress
- `TurnStarted` sets entered-turn state without bumping sequence
- heartbeat-style repeated deltas do not bump sequence
- warning / stream-error updates do not bump sequence
- terminal events produce the expected terminal snapshot state
- initial phase satisfaction yields `already_satisfied`
- later observed phase satisfaction yields `phase_matched`
- `seq_advanced` takes precedence over phase matching

Preferred test locations:

- temporary progress-law tests near [progress.rs](/home/rose/work/codex/fork/codex-rs/core/src/agent/progress.rs)
- planning doctrine in `codex-tools`
- minimal runtime wiring assertions in `codex-core`

### PR A1 — Create `codex-agent-observability` and adopt reducer ownership

Purpose:

- introduce the new crate and move live reducer ownership in one copy-free step

Move:

- progress phases
- blocker/active-work types
- snapshot types
- registry type
- reducer logic
- wait classification helpers needed by the live reducer path
- semantic interpretation of protocol-owned lifecycle status

Also do:

- workspace wiring
- `codex-core` dependency wiring
- import updates in core runtime/handler files

Invariant:

- after this PR lands, there must not be both a live core reducer and a live crate reducer claiming doctrine

Keep behavior unchanged.

### PR A2 — Wait semantics and snapshot API hardening

Purpose:

- finish the adapter-facing seam after reducer ownership moves

Update:

- [control.rs](/home/rose/work/codex/fork/codex-rs/core/src/agent/control.rs)
- [thread_manager.rs](/home/rose/work/codex/fork/codex-rs/core/src/thread_manager.rs)
- [codex.rs](/home/rose/work/codex/fork/codex-rs/core/src/codex.rs)
- [agent_progress.rs](/home/rose/work/codex/fork/codex-rs/core/src/tools/handlers/agent_progress.rs)

Core should translate runtime/lifecycle inputs, not reinterpret reducer law.

This PR should make the wait seam explicit enough to preserve current behavior:

- distinguish initial satisfaction from later phase observation
- preserve `seq_advanced` precedence over phase matching
- avoid a semantic type that treats timeout as a match reason unless that is explicitly retained for compatibility

This PR should also settle the snapshot access pattern:

- accessor methods vs intentionally public fields
- not-found / fallback construction shape

### PR A3 — Single-source progress tool contract

Purpose:

- remove drift risk between semantic enums and model-visible tool contracts

Move or centralize:

- `inspect_agent_progress` tool name
- `wait_for_agent_progress` tool name
- canonical phase-name list used in tool schema generation
- canonical blocker-reason string list
- canonical active-work-kind string list
- canonical wait-outcome or wait-match string list

Update:

- [agent_progress_tool.rs](/home/rose/work/codex/fork/codex-rs/tools/src/agent_progress_tool.rs)
- [tool_registry_plan.rs](/home/rose/work/codex/fork/codex-rs/tools/src/tool_registry_plan.rs)
- [router.rs](/home/rose/work/codex/fork/codex-rs/core/src/tools/router.rs)

### PR A4 — Make thread-spawn containment explicit in `codex-tools`

Purpose:

- replace implicit boolean containment with a named policy object

Add something like:

- `AgentToolSurfacePolicy`

Owned by:

- `codex-tools`

This policy should express:

- progress observability availability
- spawn-agent availability
- request-user-input availability
- any other current agent/session-source tool-surface inclusion rules already implied by session source

It should be constructed once from session source and feature/config inputs, then consumed by tool planning.

### PR A5 — Test taxonomy cleanup and API narrowing

Purpose:

- clean up second-doctrine tests and narrow public exports after ownership is settled

Do:

- move planning doctrine into `codex-tools` tests
- keep `codex-core` tests focused on runtime wiring
- narrow `codex-agent-observability` exports to the actual seam

This is cleanup, not the lead slice.

## Success Criteria

The family is done enough when:

- `codex-agent-observability` has no dependency on `codex-core`
- adding a new progress phase mainly touches `codex-agent-observability` plus one schema-facing adapter
- observability-owned model-visible enum/string contracts are single-sourced
- thread-spawn containment is explicit policy in `codex-tools`
- `codex-core` no longer owns progress doctrine
- core tests do not restate the module’s law surface in parallel

## Non-Goals

Do not let this family drift into:

- a generic collaboration crate
- extraction of all `AgentControl`
- moving session/runtime waiting loops into the new crate
- moving thread lifecycle ownership out of `codex-core`
- hub-style aggregation of unrelated fork features

# Compaction Policy Crate Extraction Plan

## Purpose

This document turns the current architectural diagnosis into an implementation
plan:

- keep the Codex kernel/runtime invariants in the existing upstream-shaped core
- move fork-owned compaction experimentation into a separate Rust crate
- make upstream release ingestion more predictable by concentrating fork logic
  behind a few stable seams

This is not a conceptual compaction doctrine note. It is a code-grounded plan
for how to restructure the current fork surface.

It complements:

- [docs/compaction-refresh-prune-concept-spec.md](compaction-refresh-prune-concept-spec.md)
- [docs/compaction-refresh-prune-implementation-spec.md](compaction-refresh-prune-implementation-spec.md)
- [docs/custom-fork-module-inventory.md](custom-fork-module-inventory.md)

## Problem Statement

Right now, compaction policy changes are too entangled with the inherited
engine:

- `codex-rs/core/src/compact.rs`
- `codex-rs/core/src/compact_remote.rs`
- `codex-rs/core/src/context_maintenance.rs`
- `codex-rs/core/src/continuation_bridge.rs`
- `codex-rs/core/src/governance/thread_memory.rs`
- `codex-rs/core/src/governance/prompt_layers.rs`
- `codex-rs/core/src/codex.rs`
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/config/src/config_toml.rs`
- `codex-rs/core/src/config/mod.rs`

This has three concrete costs:

1. Small policy changes force large rebuild scope.
2. Upstream ingestion sees fork behavior smeared across hot upstream files.
3. App-layer experimentation behaves like kernel surgery.

The recurring example is introducing an explicit compaction-mode split:

- `TurnBoundary`
- `IntraTurn`

This should be mostly a policy evolution. In the current shape it becomes a
cross-cutting refactor.

## Architectural Goal

Split the system into two layers.

### Kernel layer

Owned by upstream-shaped crates, primarily `codex-core`.

Responsibilities:

- task lifecycle
- model/session orchestration
- rollout persistence
- event emission
- token accounting
- protocol and `ResponseItem` invariants
- config loading and app-server/TUI plumbing

### Policy layer

Owned by a new fork crate.

Responsibilities:

- compaction mode semantics
- engine/profile selection semantics
- between-turn vs intra-turn rules
- artifact family selection
- continuation-bridge shaping
- thread-memory shaping
- retention/prune rules
- prompt/spec assembly for maintenance artifacts
- replacement-history shaping rules

The kernel asks policy:

- what mode is this maintenance action?
- which artifacts should be generated?
- how should rebuilt history be shaped?

The policy layer must not own session state or persistence.

## Proposed New Crate

Create a new workspace crate:

- `codex-context-maintenance-policy`

This name is intentionally narrow. It should own compaction/refresh/prune
policy, not become a generic dump for all fork behavior.

### Why this crate boundary

It gives us:

- a smaller rebuild surface for compaction experiments
- a stable fork-owned home for policy logic
- a cleaner upstream ingest story
- a path to add more profiles/engines without rewriting `codex-core`

## What Stays In `codex-core`

The following should remain in `codex-core` or other upstream-shaped runtime
crates:

- `CompactTask` and task spawning
- app-server RPC handlers for `thread/compact/start`, `thread/refresh/start`,
  `thread/prune/start`
- TUI and config write plumbing
- `Session` mutation and history replacement
- model client/session creation and streaming
- rollout persistence (`CompactedItem`, history replacement, event emission)
- token recomputation
- protocol item types (`ResponseItem`, `CompactedItem`, etc.)

In other words: `codex-core` remains the executor and persistence engine.

## What Moves To The New Crate

The new policy crate should own the following logic.

### 1. Maintenance mode semantics

Define explicit maintenance mode:

- `TurnBoundary`
- `IntraTurn`

And action kind:

- `Compact`
- `Refresh`
- `Prune`

This is the main policy axis missing today.

### 2. Engine semantics

Define engine semantics independent of provider:

- `RemoteVanilla`
- `RemoteHybrid`
- `LocalPure`

`codex-core` should stop deciding compaction strategy from provider class and
instead follow policy selection.

### 3. Artifact policy

The crate decides, by mode and engine:

- whether to generate `thread_memory`
- whether to generate `continuation_bridge`
- whether previous `thread_memory` is canonical base
- whether prior `continuation_bridge` is carried or dropped
- whether legacy `ResponseItem::Compaction` markers are preserved or stripped

### 4. Retention rules

Own policy for:

- raw message retention windows
- hot/warm tail rules
- prune eligibility
- stale artifact removal
- turn-boundary carry-forward rules

### 5. Prompt/spec assembly

The crate should own the artifact request specs for:

- continuation bridge prompt building
- thread memory prompt building
- schema/variant selection
- mode-specific prompt envelopes

This keeps prompt experimentation in the fork crate instead of `core`.

### 6. Replacement-history shaping

The crate should own the deterministic logic for:

- where authoritative artifacts are inserted
- where summary or compaction markers belong
- when initial context should appear
- what to drop in local-pure mode vs hybrid mode

## First-Class Policy Contract

The new crate should be built around a small typed contract.

### Input side

`codex-core` passes a pure request object, for example:

```rust
pub struct MaintenancePolicyRequest {
    pub action: MaintenanceAction,
    pub mode: MaintenanceMode,
    pub engine: CompactionEngine,
    pub governance_variant: GovernancePathVariant,
    pub profile: MaintenanceProfile,
    pub prior_history: Vec<ResponseItem>,
    pub prior_thread_memory: Option<String>,
    pub prior_bridge: Option<String>,
    pub initial_context: Vec<ResponseItem>,
}
```

This type should stay free of `Session`, `TurnContext`, and model-client
objects.

### Output side

The policy crate returns a plan, not side effects:

```rust
pub struct MaintenancePolicyPlan {
    pub artifact_requests: Vec<ArtifactRequest>,
    pub history_shape: HistoryShapePlan,
    pub carry_forward_rules: CarryForwardPlan,
}
```

After model-backed artifacts are generated by `codex-core`, policy gets a second
pure pass:

```rust
pub struct MaintenanceAssemblyRequest {
    pub policy_plan: MaintenancePolicyPlan,
    pub generated_artifacts: GeneratedArtifacts,
}
```

And returns:

```rust
pub struct MaintenanceAssemblyResult {
    pub replacement_history: Vec<ResponseItem>,
    pub compacted_message: String,
    pub reference_context_policy: ReferenceContextPolicy,
}
```

This two-stage design is important because it keeps prompt/artifact decisions in
the policy crate while leaving model execution in `codex-core`.

## Proposed Module Layout

Inside `codex-context-maintenance-policy`:

- `mode.rs`
  - `MaintenanceMode`
  - `MaintenanceAction`
  - `MaintenanceProfile`
- `engine.rs`
  - engine semantics
- `artifacts.rs`
  - artifact selection rules
- `bridge.rs`
  - continuation bridge prompt/spec builders
- `thread_memory.rs`
  - thread memory prompt/spec builders
- `retention.rs`
  - raw-tail and prune rules
- `history_shape.rs`
  - insertion/removal placement rules
- `plan.rs`
  - request/plan/result types
- `assemble.rs`
  - rebuild final replacement history from generated artifacts

The crate should depend on:

- `codex-protocol`
- `codex-config` only if necessary for shared enums
- small utility crates as needed

It should not depend on `codex-core`.

## Minimal Seams Required In Upstream-Shaped Code

The goal is not zero touches in `core`; it is minimal stable touches.

### Seam 1: maintenance dispatch

`codex-core` keeps the existing task entry points, but calls into the policy
crate for plan/assembly.

Primary touch files:

- `codex-rs/core/src/compact.rs`
- `codex-rs/core/src/compact_remote.rs`
- `codex-rs/core/src/context_maintenance.rs`

### Seam 2: model-backed artifact runner

`codex-core` provides a thin adapter that can execute an `ArtifactRequest` and
return structured text/JSON.

This adapter remains local to `core` because it needs:

- `Session`
- `TurnContext`
- `ModelClientSession`
- telemetry and rate-limit handling

### Seam 3: config-to-policy translation

Config stays where it is, but `core` translates it into the policy crate’s
request types.

Primary touch files:

- `codex-rs/config/src/config_toml.rs`
- `codex-rs/core/src/config/mod.rs`

### Seam 4: UI/config controls

TUI/app-server only need to read/write stable config fields.

They should not know detailed policy internals.

Primary touch files:

- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/tui/src/bottom_pane/memories_settings_view.rs`
- `codex-rs/tui/src/app_server_session.rs`

## Explicit Turn-Boundary vs Intra-Turn Law

This is the main semantic change the extraction should make easy.

### Intra-turn

Question answered:

- what does the currently running turn need in order to survive context
  collapse and continue?

Rules:

- generate `continuation_bridge`
- do not rewrite durable `thread_memory` as committed truth
- may read current durable `thread_memory`
- aggressively trim transient raw history
- prune previous bridge when superseded

### Turn-boundary

Question answered:

- what stable thread state should survive into later turns?

Rules:

- update `thread_memory`
- do not preserve the previous intra-turn bridge as durable memory
- write a clean post-turn carry-forward state
- drop transient turn-scoped execution posture

This split should live in the policy crate, not as scattered conditionals in
`core`.

## Migration Strategy

Do this in phases so the system stays runnable throughout.

### Phase 1: create the crate and move pure policy types

Move only:

- `MaintenanceMode`
- `MaintenanceAction`
- engine/profile enums and policy plan/result types
- retention and insertion rules that can already be pure

Keep existing generators in `core`.

Goal:

- establish the crate boundary with minimal behavior change

### Phase 2: move history-shaping logic

Move into the policy crate:

- insertion placement helpers
- raw retention helpers
- stale artifact filtering
- local-pure history-shaping rules

This should pull logic out of:

- `codex-rs/core/src/compact.rs`
- `codex-rs/core/src/context_maintenance.rs`

### Phase 3: move bridge/thread-memory prompt assembly

Move prompt/spec construction for:

- `continuation_bridge`
- `thread_memory`

into the policy crate.

Keep model execution in `core`.

Goal:

- prompt experiments stop requiring direct edits in `codex-core`

### Phase 4: make mode split authoritative

Implement the actual semantic split:

- `TurnBoundary`
- `IntraTurn`

and route:

- auto-compaction
- manual `/compact`
- `/refresh`
- `/prune`

through explicit policy requests.

### Phase 5: strip legacy carried compaction markers in local-pure mode

Add explicit policy:

- local-pure does not preserve legacy `ResponseItem::Compaction` items unless
  deliberately configured

This cleans the local history shape and removes confusion over “encrypted
compaction” artifacts in pure-local runs.

## Concrete File Targets For Phase 1

### New files

- `codex-rs/context-maintenance-policy/Cargo.toml`
- `codex-rs/context-maintenance-policy/src/lib.rs`
- `codex-rs/context-maintenance-policy/src/mode.rs`
- `codex-rs/context-maintenance-policy/src/engine.rs`
- `codex-rs/context-maintenance-policy/src/plan.rs`
- `codex-rs/context-maintenance-policy/src/history_shape.rs`
- `codex-rs/context-maintenance-policy/src/retention.rs`

### Existing files to touch minimally

- `codex-rs/Cargo.toml`
- `codex-rs/core/Cargo.toml`
- `codex-rs/core/src/compact.rs`
- `codex-rs/core/src/context_maintenance.rs`

## Upstream Ingest Benefit

This is the main long-term payoff.

After extraction:

- upstream changes in `codex-core` are less likely to collide with fork policy
  experimentation
- most fork compaction work lives in one owned crate
- update review focuses on a few seam files instead of a smeared custom surface

Ingest shape becomes:

1. update `upstream-main`
2. ingest onto fork `main`
3. revalidate seam files
4. adjust fork policy crate only if hook contracts changed

That is much more predictable than the current model.

## Testing Strategy

### Policy crate tests

The new crate should have dense unit coverage for:

- mode semantics
- artifact selection
- history insertion ordering
- prune and retention rules
- local-pure vs hybrid shaping

These tests should be fast and independent of live model execution.

### Core seam tests

Keep a smaller set of integration tests in `codex-core` for:

- task dispatch still works
- model-backed artifact requests still round-trip
- replacement history is applied correctly
- app-server/TUI settings still drive the selected engine/profile

This shifts most compaction testing away from heavy `core` integration tests and
into a faster policy crate.

## Non-Goals

- moving all compaction execution out of `codex-core`
- changing protocol item definitions out of `codex-protocol`
- turning the new crate into a generic fork playground
- rewriting upstream TUI/app-server architecture
- solving every governance-path concern in the same extraction

## Definition Of Done

This extraction is successful when:

1. `TurnBoundary` vs `IntraTurn` is represented as an explicit typed policy
   input.
2. Most compaction experimentation happens in the new crate rather than
   `codex-core`.
3. `codex-core` keeps only execution/persistence hooks and thin adapters.
4. Upstream ingest for compaction changes is concentrated in a small seam set.
5. Rebuild scope for compaction-only changes is materially reduced.

## Recommended First PR Sequence

1. `codex/context-maintenance-policy-p1-scaffold`
   - add crate
   - add request/plan/result types
   - move pure insertion/retention helpers

2. `codex/context-maintenance-policy-p2-history-shaping`
   - route local compaction and refresh/prune history assembly through the crate

3. `codex/context-maintenance-policy-p3-artifact-prompts`
   - move bridge/thread-memory prompt/spec assembly

4. `codex/context-maintenance-policy-p4-mode-split`
   - make `TurnBoundary` vs `IntraTurn` authoritative

5. `codex/context-maintenance-policy-p5-local-cleanup`
   - strip legacy carried compaction markers in local-pure mode
   - tighten post-turn vs intra-turn artifact lifetimes

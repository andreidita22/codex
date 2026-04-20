# Compaction Policy Thread-Memory Current Semantics

## Purpose

This document records the current semantics of the thread-memory-related control
surfaces in the context-maintenance system.

It exists to make the current behavior explicit before any future semantic
changes. In particular, it distinguishes:

- route/timing-driven inactivity
- governance-driven suppression
- persisted thread memory mode

These are related, but they are not the same thing.

## Executive Read

Current live behavior is:

- mid-turn maintenance does **not** generate `ThreadMemory`
- turn-boundary maintenance may generate `ThreadMemory`
- `ThreadMemoryGovernance::Disabled` suppresses **new**
  `ThreadMemory` generation when a route would otherwise request it
- `ThreadMemoryGovernance::Disabled` does **not** currently purge prior
  `ThreadMemory` or disable retention that keys off existing `ThreadMemory`
- `ThreadMemoryMode::Disabled` is a separate protocol/session surface and
  should not be conflated with route timing

If the intended product rule is:

- use `ThreadMemory` only between turns
- use `ContinuationBridge` mid-turn

then the current route matrix already expresses that rule without needing a
special disabled state.

## Control Surfaces

### 1. Timing-Driven Route Activation

The primary semantic split lives in the maintenance route matrix:

- `Compact + IntraTurn` requests `ContinuationBridge`
- `Compact + TurnBoundary` may request `ThreadMemory`
- `Refresh + TurnBoundary` may request `ThreadMemory`
- `Prune + TurnBoundary` does not request `ThreadMemory`

Relevant owner:

- [route_matrix.rs](../../codex-rs/context-maintenance-policy/src/route_matrix.rs)

This means `ThreadMemory` is already inactive mid-turn **by timing law**.

That is different from saying thread memory is disabled globally or for the
entire thread.

### 2. Governance Overlay

The context-maintenance adapter maps core governance config into the policy
crate:

- `GovernancePathVariant::Off` ->
  `ThreadMemoryGovernance::Disabled`
- `GovernancePathVariant::StrictV1Shadow` ->
  `ThreadMemoryGovernance::Enabled`
- `GovernancePathVariant::StrictV1Enforce` ->
  `ThreadMemoryGovernance::Enabled`

Relevant owner:

- [context_maintenance_runtime.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance_runtime.rs)

The policy route matrix then applies a governance overlay:

- if governance is `Disabled`, remove requested `ArtifactKind::ThreadMemory`
  from the selected route plan
- record `GovernanceEffect::ThreadMemorySuppressed`

Relevant owner:

- [route_matrix.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/route_matrix.rs)

### 3. Persisted Thread Memory Mode

There is also a distinct protocol/session-level control surface:

- [ThreadMemoryMode](/home/rose/work/codex/fork/codex-rs/protocol/src/protocol.rs)

Values:

- `Enabled`
- `Disabled`

This mode is persisted in rollout/session metadata and should be treated as a
separate feature/configuration surface from the policy route matrix.

Relevant owners:

- [protocol.rs](/home/rose/work/codex/fork/codex-rs/protocol/src/protocol.rs)
- [codex.rs](/home/rose/work/codex/fork/codex-rs/core/src/codex.rs)

## Current Route Semantics

### Intra-Turn Compact

Current behavior:

- request `ContinuationBridge`
- do not request `ThreadMemory`
- inject context before the last real user or summary

Interpretation:

- thread memory is inactive here because this route does not use it
- this is the intended mid-turn path

### Turn-Boundary Compact

Current behavior on supported engines:

- request `ThreadMemory`
- do not request `ContinuationBridge`
- no special initial-context injection

Interpretation:

- this is the primary durable thread-memory generation path

### Turn-Boundary Refresh

Current behavior on supported engines:

- request `ThreadMemory`
- prune superseded artifacts
- do not request `ContinuationBridge`

Interpretation:

- refresh is a turn-boundary thread-memory maintenance path

### Turn-Boundary Prune

Current behavior:

- request `PruneManifest`
- do not request new `ThreadMemory`
- drop `PruneManifest` and `ContinuationBridge`
- retain a recent raw tail when final history contains `ThreadMemory`

Interpretation:

- prune does not generate thread memory
- but existing thread memory may still affect retention behavior

## Current Meaning Of `ThreadMemoryGovernance::Disabled`

Today, `ThreadMemoryGovernance::Disabled` means:

- if a route would request new `ThreadMemory`, suppress that request

Today, it does **not** mean:

- purge existing `ThreadMemory` artifacts from history
- add `ArtifactKind::ThreadMemory` to artifact-drop behavior
- disable retention gated on final history containing `ThreadMemory`
- pretend prior thread memory never existed

So the current semantics are closer to:

- "do not generate new thread memory"

than to:

- "thread memory is fully disabled everywhere"

## Can This Enter Mid-Thread?

Yes.

The governance value is read when the runtime builds a maintenance plan, not
only when the thread is created.

Relevant owners:

- [TurnContext::governance_path_variant()](/home/rose/work/codex/fork/codex-rs/core/src/codex.rs)
- [runtime_plan_for_compact(...)](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance_runtime.rs)
- [runtime_plan_for_turn_boundary_maintenance(...)](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance_runtime.rs)

This means a governance change can affect the next:

- `/compact`
- `/refresh`
- `/prune`

It does **not** retroactively rewrite prior history at the moment the setting
changes.

## "Inactive" Versus "Disabled"

For thread memory, these should be read differently:

### Inactive

Meaning:

- the current route/timing does not use `ThreadMemory`

Example:

- intra-turn compact

This is a normal consequence of timing law.

### Disabled

Meaning today:

- governance suppresses new `ThreadMemory` generation even on routes that would
  otherwise request it

This is stronger than inactivity, but weaker than a full purge or ignore-all
semantics.

## Practical Current Guidance

If the intended behavior is:

- use `ThreadMemory` between turns only
- use `ContinuationBridge` mid-turn only

then:

- keep governance enabled
- rely on timing law
- do **not** use `ThreadMemoryGovernance::Disabled` to express ordinary
  mid-turn inactivity

`ThreadMemoryGovernance::Disabled` should currently be treated as a special
override that suppresses new thread-memory generation on otherwise eligible
routes.

## Known Semantic Ambiguity

The remaining ambiguity is this:

- should `ThreadMemoryGovernance::Disabled` keep meaning
  "suppress generation only"
- or should it mean
  "drop and ignore existing thread memory too"

Current implementation chooses the first meaning.

That is acceptable if governance-off is used mainly for:

- new chats
- forward-only suppression

If future product behavior needs a true kill switch for existing durable thread
memory, this document should be updated together with the route/governance
implementation.

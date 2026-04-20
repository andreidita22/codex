# Compaction/Refresh/Prune Concept Spec (v0.2)

## Scope

This spec defines a three-command continuity model for long-running Codex threads:

- `/compact` for budget survival
- `/refresh` for semantic continuity quality
- `/prune` for deterministic footprint hygiene

It also defines authority ordering across retained artifacts so compaction projections do not silently outrank durable state.

## Artifact Model

### A1: Continuation bridge (frontier baton)

- Purpose: immediate handoff for the next active continuation step.
- Characteristics:
  - short-lived
  - next-action oriented
  - derived from current durable state + recent frontier
- Authority: lower than `A2`, higher than raw transcript recap for immediate continuation.
- Non-goal: must not become a second long-memory store.

### A2: Thread memory (ODEU durable continuity)

- Purpose: authoritative durable continuity substrate.
- Minimum carried content:
  - ODEU durable state
  - active workflow/operator state
  - retained witness references
  - unresolved/blocking states
  - pending escalation/reproposal items
- Authority: authoritative continuity source.

### A3: Compaction summary/projection

- Purpose: model-facing compressed projection of prior context.
- Characteristics:
  - derived
  - can be stale
  - may be regenerated or dropped
- Authority: non-authoritative. Never outranks `A1`/`A2`.

### A4: Raw transcript tail (evidence carry)

- Purpose: bounded verbatim carryover for recent context evidence.
- Structure:
  - hot tail: must-retain zone
  - warm tail: prune-eligible zone
- Authority: evidence only; not governing state.

### A5: Undo/ghost continuity artifacts

- Purpose: rollback/fork/undo continuity support.
- Carry rule:
  - preserved
  - not injected into default reasoning context unless rollback/recovery paths are active.

### A6: Effective runtime control state

- Purpose: authoritative execution controls at turn/runtime boundary.
- Includes effective state such as:
  - model/reasoning selections
  - permissions/sandbox posture
  - relevant runtime mode toggles
- Optional: recent delta log for diagnostics.
- Authority: authoritative for execution controls.

## Authority Order

1. `A6` effective runtime control state (execution controls)
2. `A2` durable thread memory (continuity)
3. `A1` continuation bridge (frontier baton)
4. `A3` summary projection (derived)
5. `A4` raw tail (evidence)
6. `A5` undo/ghost (continuity asset, default-non-injected)

Operational law:

- `A2` is durable truth.
- `A1` is frontier truth for next-step execution.
- `A3` is derivative and must never override `A1/A2`.

## Command Semantics

## `/compact`

Single user command with two internal modes.

### `/compact` micro (intra-turn)

- Goal: preserve liveness under budget pressure while work is still in progress.
- Actions:
  - regenerate/update `A1`
  - aggressively trim `A4`
  - optionally refresh `A3`
  - preserve `A5`
- Constraint:
  - do not blindly commit a full durable `A2` rewrite as if turn was closed.

### `/compact` macro (between turns)

- Goal: durable continuity rebasing at turn boundary.
- Actions:
  - regenerate/commit `A1 + A2 + A3`
  - prune `A4` by profile policy
  - preserve `A5`
  - keep `A6` effective state coherent

## `/refresh` (between turns only)

- Goal: improve semantic continuity quality without forcing full compaction rewrite.
- Actions:
  - regenerate `A1`
  - regenerate/normalize `A2`
  - revalidate `A3` against current `A2`
- `A3` validity rule:
  - if material `A2` change invalidates `A3`, then either:
    - regenerate `A3`, or
    - mark/drop stale `A3`.

## `/prune` (between turns only)

- Goal: deterministic footprint reduction without semantic reinterpretation.
- Actions:
  - prune only eligible footprint content (`A4` warm zone, stale/superseded derived items)
  - no semantic regeneration of `A1/A2`
  - preserve `A5`, preserve effective `A6`
- Must emit a deterministic prune manifest:
  - what was pruned
  - why it was prune-eligible
  - what superseded it (if applicable)

## Use Cases

### Use `/prune` when

- You want fast, low-latency hygiene during long threads.
- You do not want semantic re-summarization.
- You need deterministic cleanup before/after bursts of tool output.

### Use `/refresh` when

- You want a better baton before delegation/model switch/checkpoint.
- Continuity quality matters more than immediate token emergency.
- You need to reconcile and normalize durable thread state.

### Use `/compact` when

- Token pressure is near or over limit.
- Auto-compaction fires during active execution.
- Thread drift is large and full context survival rewrite is necessary.

## Profile Direction (not final numbers)

- `spark-lean`: smallest `A4`, terse `A1/A2`, strict derived-artifact trimming.
- `standard`: moderate `A4`, balanced continuity quality.
- `deep`: larger `A4`, richer `A1/A2`.

Retention numbers are intentionally deferred until the authority model and micro-vs-macro `/compact` law are implemented.

## Invariants

- Lower-layer derived artifacts must not silently override higher-authority continuity state.
- Intra-turn reduction is for liveness; between-turn reduction is for durability.
- `/prune` changes footprint, not meaning.
- Rollback artifacts are preserved without contaminating default reasoning context.

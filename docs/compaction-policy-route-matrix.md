# Compaction Policy Route Matrix

## Purpose

This document is the authoritative route table for context-maintenance behavior
during the planned `codex-context-maintenance-policy` extraction.

It exists to prevent semantic drift while code moves from `codex-core` into the
new policy crate.

It complements:

- [docs/compaction-policy-crate-extraction-plan.md](compaction-policy-crate-extraction-plan.md)

This document describes the **target semantics** the extraction should converge
to. Early migration steps such as `p0` may still record known deltas where the
current implementation has not yet been brought into line, but those deltas
must be made explicit rather than silently folded into this table.

## Axes

Each route is defined by three axes:

- `MaintenanceAction`
  - `Compact`
  - `Refresh`
  - `Prune`
- `MaintenanceTiming`
  - `TurnBoundary`
  - `IntraTurn`
- `CompactionEngine`
  - `RemoteVanilla`
  - `RemoteHybrid`
  - `LocalPure`

## Artifact Lifetime Law

The route matrix is governed by these lifetime defaults:

- `continuation_bridge`
  - `TurnScoped`
- `thread_memory`
  - `DurableAcrossTurns`
- prune manifest / compaction marker
  - `MarkerOnly`

Operational consequences:

- turn-scoped artifacts may help a running turn survive compaction, but must not
  be treated as durable thread memory
- durable artifacts survive into later turns
- marker artifacts may persist as metadata/history markers but do not govern
  continuation semantics

## Context Injection Law

Context injection is a first-class policy choice, not just an assembly detail.

- `None`
  - do not inject canonical initial context into replacement history
- `BeforeLastRealUserOrSummary`
  - inject canonical initial context before the last real user item, or before
    the final summary-like user item when no real user item remains

Default rule:

- `IntraTurn` routes use `BeforeLastRealUserOrSummary`
- `TurnBoundary` routes use `None`

## Route Table

| Action | Timing | Engine | Generate `thread_memory` | Generate `continuation_bridge` | Inject initial context | Preserve previous bridge | Preserve legacy `ResponseItem::Compaction` | Durable result after completion |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `Compact` | `IntraTurn` | `LocalPure` | no | yes | `BeforeLastRealUserOrSummary` | no | no by default | bridge only |
| `Compact` | `IntraTurn` | `RemoteHybrid` | no | yes | `BeforeLastRealUserOrSummary` | no | preserve only if upstream path still requires marker continuity | bridge only |
| `Compact` | `IntraTurn` | `RemoteVanilla` | no | no | `BeforeLastRealUserOrSummary` | no | yes, upstream-owned | upstream compaction marker only |
| `Compact` | `TurnBoundary` | `LocalPure` | yes | no durable carry-forward | `None` | no | no by default | updated durable `thread_memory` |
| `Compact` | `TurnBoundary` | `RemoteHybrid` | yes | no durable carry-forward | `None` | no | preserve only where required for upstream compatibility | updated durable `thread_memory` plus upstream summary/marker |
| `Compact` | `TurnBoundary` | `RemoteVanilla` | no | no | `None` | no | yes, upstream-owned | upstream summary/marker only |
| `Refresh` | `TurnBoundary` | `LocalPure` | yes | no | `None` | no | no | updated durable `thread_memory` |
| `Refresh` | `TurnBoundary` | `RemoteHybrid` | yes | no | `None` | no | no | updated durable `thread_memory` |
| `Refresh` | `TurnBoundary` | `RemoteVanilla` | no | no | `None` | no | no | no semantic regeneration; route invalid unless engine selection is switched to a fork-owned path |
| `Prune` | `TurnBoundary` | `LocalPure` | no | no | `None` | no | no | prune manifest only |
| `Prune` | `TurnBoundary` | `RemoteHybrid` | no | no | `None` | no | no | prune manifest only |
| `Prune` | `TurnBoundary` | `RemoteVanilla` | no | no | `None` | no | no | prune manifest only |

## Unsupported Routes

The following routes are intentionally unsupported and should fail closed rather
than invent semantics implicitly:

- `Refresh + IntraTurn + any engine`
- `Prune + IntraTurn + any engine`

Reason:

- `Refresh` is a durable continuity recomputation operation, not a live-turn
  survival operation
- `Prune` is deterministic boundary hygiene, not a mid-turn rescue mechanism

If future product requirements need one of these routes, the matrix should be
updated first and the new route should be treated as a new semantic addition,
not as a fallback.

## Route Interpretations

### `Compact + IntraTurn`

Question answered:

- what does the currently running turn need in order to survive context
  collapse and continue?

Rules:

- generate `continuation_bridge`
- do not rewrite durable `thread_memory`
- may read existing durable `thread_memory`
- aggressively trim transient raw history
- previous bridge is superseded

### `Compact + TurnBoundary`

Question answered:

- what stable state should survive into later turns?

Rules:

- update durable `thread_memory` when the selected engine/path supports it
- do not preserve turn-scoped bridge as durable memory
- clear transient execution posture

### `Refresh + TurnBoundary`

Question answered:

- can we recompute durable continuity state without running full compaction?

Rules:

- recompute durable memory state
- do not generate a bridge
- should not be used as a substitute for intra-turn survival

### `Prune + TurnBoundary`

Question answered:

- what can be removed deterministically without changing carried meaning?

Rules:

- no new semantic artifact generation
- emit prune manifest
- prune stale/duplicated/transient items only

## Fixture Expectations

The eventual policy crate should have fixtures or table-driven tests that assert
at least the following for each route:

- which artifacts are requested
- which artifacts are forbidden
- which context injection policy applies
- whether legacy compaction markers are retained or stripped
- what artifact lifetime is assigned to each generated artifact
- whether the output is intended to be durable across turns

Prune-manifest handling should be treated as an explicit artifact family during
implementation. If there is no dedicated `prune_manifest.rs`, it should live
under `artifact_codecs.rs` rather than being left as an implicit side effect.

## Current Migration Use

This document should be treated as the semantic source of truth during:

1. `p0` route-matrix work
2. `p1` crate scaffold
3. `p2` history-shaping extraction
4. `p3` artifact codec and prompt extraction
5. `p4` timing-law activation

If implementation pressure reveals a route that should differ from this matrix,
the matrix should be updated first, then code should be changed to match it.

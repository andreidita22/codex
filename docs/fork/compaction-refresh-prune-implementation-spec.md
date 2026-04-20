# Compaction/Refresh/Prune Implementation Spec (Code-Grounded, v0.2)

## Purpose

This spec maps the conceptual split (`/compact`, `/refresh`, `/prune`) to concrete integration points in the current `codex-rs` codebase, with minimal-touch changes and explicit behavior boundaries for intra-turn vs between-turn flows.

## Baseline: Current Code Paths

## Slash/UI dispatch

- Slash enum and descriptions:
  - `codex-rs/tui/src/slash_command.rs`
- Slash command execution:
  - `codex-rs/tui/src/chatwidget.rs` (`dispatch_command`)
- TUI command wrapper:
  - `codex-rs/tui/src/app_command.rs`
- Existing `/compact` send path:
  - `codex-rs/tui/src/app_event_sender.rs` (`compact()`)

## Submission/Op routing

- Protocol operation enum:
  - `codex-rs/protocol/src/protocol.rs` (`enum Op`)
- Core submission dispatcher:
  - `codex-rs/core/src/codex.rs` (main `match` over `Op`)
- Existing compact handler:
  - `codex-rs/core/src/codex.rs` (`handlers::compact`)

## Existing compact execution

- Session task:
  - `codex-rs/core/src/tasks/compact.rs` (`CompactTask`)
- Local compact implementation:
  - `codex-rs/core/src/compact.rs`
- Remote compact implementation:
  - `codex-rs/core/src/compact_remote.rs`
- Auto-compaction trigger and intra-turn compact loop:
  - `codex-rs/core/src/codex.rs` (`run_pre_sampling_compact`, `run_auto_compact`)

## Existing artifact insertion behavior

- Continuation bridge generation/insertion:
  - local + remote compaction paths
- Thread memory generation/insertion (strict/governance-gated):
  - local + remote compaction paths
- Raw retained transcript trimming for governed compaction:
  - `retain_recent_raw_conversation_messages` in `compact.rs`

Current state already provides the right insertion seam for authoritative artifacts; this proposal extends maintenance semantics around that seam.

## Target Behavior Split

## `/compact`

- Keep current user-facing command.
- Internal behavior split:
  - intra-turn (micro): liveness-focused compact semantics
  - between-turn/manual (macro): durable continuity semantics

## `/refresh`

- New between-turn-only maintenance command.
- Regenerates continuity artifacts (`A1`, `A2`), revalidates projection (`A3`), does light deterministic cleanup.

## `/prune`

- New between-turn-only deterministic cleanup command.
- No semantic regeneration.
- Must emit prune manifest metadata.

## Proposed Code Changes

## 1) Protocol additions

File:
- `codex-rs/protocol/src/protocol.rs`

Add new ops:
- `Op::RefreshContext`
- `Op::PruneContext`

Update `Op::kind()` string mapping accordingly.

## 2) TUI slash commands

Files:
- `codex-rs/tui/src/slash_command.rs`
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/tui/src/app_command.rs`
- `codex-rs/tui/src/app_event_sender.rs`

Add:
- `SlashCommand::Refresh`
- `SlashCommand::Prune`

Rules:
- `available_during_task = false` for both commands.
- Dispatch to new ops.
- Keep `/compact` semantics unchanged in UI.

## 3) Core op handlers

File:
- `codex-rs/core/src/codex.rs`

Add handler entry points:
- `handlers::refresh_context(sess, sub_id)`
- `handlers::prune_context(sess, sub_id)`

Both should spawn task-backed flows (same lifecycle discipline as other standalone tasks).

## 4) New maintenance task module

Files:
- `codex-rs/core/src/tasks/mod.rs`
- new: `codex-rs/core/src/tasks/context_maintenance.rs`

Introduce:
- `ContextMaintenanceTask`
- `ContextMaintenanceMode::{Refresh, Prune}`

Task kind:
- `TaskKind::Regular` (between-turn maintenance command; not a compact task)

Rationale:
- avoids overloading `CompactTask`
- isolates maintenance behavior from auto-compaction semantics

## 5) New maintenance engine module

New file:
- `codex-rs/core/src/context_maintenance.rs`

Primary API:
- `run_refresh(sess, turn_context) -> CodexResult<()>`
- `run_prune(sess, turn_context) -> CodexResult<()>`

Shared helpers:
- history scan/classification
- deterministic prune candidate selection
- artifact metadata validation
- history replacement/writeback wrapper

## Artifact Validity / Metadata Rules (Implementation)

## A3 validity stamp

Add lightweight metadata stamp for projection validation, for example:
- `a2_digest`
- `schema_version`
- `generated_at_turn_id` or sequence marker

Validation point:
- refresh path

Rule:
- if `A2` changed materially and `A3` stamp mismatches, regenerate or drop `A3`.

## Prune manifest emission

Emit a structured manifest artifact for `/prune` runs:
- items removed
- reason class
- superseded-by reference when available

Storage strategy:
- as tagged `developer` message in replacement history
- compact format, deterministic ordering

## A5 injection boundary

Rule:
- preserve `A5` in stored history
- exclude from default active model prompt injection unless explicit rollback/recovery flow is active

Implementation point:
- prompt-building filters in maintenance and compaction pathways must respect this default exclusion.

## Intra-turn vs Between-turn Compact Law (Code)

## Intra-turn (micro compact)

Path:
- `run_auto_compact(..., InitialContextInjection::BeforeLastUserMessage)` and related calls

Behavior constraint:
- do not perform full durable `A2` rewrite commit in this mode.
- allow `A1` regeneration and `A4` pressure relief.
- preserve model liveness and execution continuity.

## Between-turn (macro compact)

Path:
- manual `/compact` and pre-turn compact paths (`InitialContextInjection::DoNotInject`)

Behavior:
- full durable continuity regeneration (`A1/A2/A3`) allowed.

## Profile System (Lean/Standard/Deep)

Goal:
- allow model-family specific retention behavior (especially Spark 128k).

Initial config surface (proposed):
- `context_maintenance_profile = "spark_lean" | "standard" | "deep"`
- optional per-command overrides later.

Mapping targets:
- `A4` hot/warm tail bounds
- `A1/A2` verbosity caps
- prune aggressiveness

Implementation location candidates:
- `codex-rs/core/src/config/mod.rs`
- `codex-rs/core/config.schema.json` (regenerated after config changes)

## Testing Plan

## Unit tests

New or extended tests for:
- deterministic prune candidate selection
- A3 validity-stamp logic
- A5 injection filter behavior

Likely files:
- new `codex-rs/core/src/context_maintenance_tests.rs`
- focused tests in `compact_tests.rs` for micro/macro distinction

## Integration tests

Extend suite coverage:
- slash command dispatch (`tui` tests)
- new op routing
- refresh/prune history shape assertions
- prune manifest presence/shape
- spark-lean profile retention behavior

Likely files:
- `codex-rs/tui/src/chatwidget/tests/slash_commands.rs`
- `codex-rs/core/tests/suite/compact.rs`
- `codex-rs/core/tests/suite/compact_remote.rs` (guard against unintended remote-path regressions)

## Rollout Plan

1. Add protocol + slash + handler plumbing.
2. Land deterministic `/prune` first (no semantic regen).
3. Land `/refresh` with `A1/A2` regen and `A3` validation.
4. Enforce micro vs macro `/compact` law in compaction codepaths.
5. Add profile gate (`spark_lean`, `standard`, `deep`) and test matrix.

## Explicit Non-Goals (this phase)

- Replacing remote `/responses/compact` behavior.
- Replacing existing continuation-bridge or thread-memory schemas in this first pass.
- UI-level profile pickers in app-server surfaces (can follow after core semantics stabilize).

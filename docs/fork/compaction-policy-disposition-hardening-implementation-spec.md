# Compaction Policy Disposition Hardening Implementation Spec

## Purpose

This spec defines a small focused follow-up for context maintenance after the
main extraction and hardening families.

It addresses two remaining worthwhile items from the fourth audit:

1. harden the `apply_history_disposition` API so summary-retention law cannot
   be bypassed accidentally
2. add explicit local-compact invariant coverage for disposition behavior that
   is currently achieved by construction rather than by the shared helper path

This spec is intentionally narrow. It should not reopen architecture or change
governance semantics.

## Read With

- [compaction-policy-post-pr6-followup-spec.md](/home/rose/work/codex/fork/docs/fork/compaction-policy-post-pr6-followup-spec.md)
- [compaction-policy-thread-memory-current-semantics.md](/home/rose/work/codex/fork/docs/fork/compaction-policy-thread-memory-current-semantics.md)
- [gptpro-audit-04-custom-modules-and-watchplan.md](/home/rose/work/codex/fork/docs/fork/gptpro-audit-04-custom-modules-and-watchplan.md)

## Executive Read

The context-maintenance boundary is already structurally strong.

What remains here is not another extraction pass. It is a small hardening step
to:

- remove an avoidable public footgun in the policy crate
- lock the behavior-by-construction invariants for local compact so future
  policy changes cannot silently drift away from the actual local replacement
  history path

## Scope

This slice includes:

- `apply_history_disposition` API hardening
- local compact disposition invariant tests

This slice does **not** include:

- changing `ThreadMemoryGovernance::Disabled` semantics
- rewriting local compact to execute the shared disposition helper
- changing route law
- changing retention law
- changing remote compact behavior
- reducing broader core test doctrine outside the local compact invariants

## Current Problems

### 1. Public Disposition Footgun

Current state:

- `apply_history_disposition(...)` is public
- it accepts `SummaryDispositionPolicy::KeepLatestCompactionSummary`
- it only guards misuse with `debug_assert!`
- in release builds, a misuse would silently behave as if no summary matched

Production callers appear correct today, but the API shape is weaker than the
law it represents.

### 2. Local Compact Is Correct By Construction, But Under-Locked

Current state:

- refresh, prune, and remote compact visibly run through shared policy-owned
  disposition paths
- local compact still assembles replacement history directly in
  `codex-rs/core/src/compact.rs`

That is acceptable if the resulting history still satisfies the declared route
law. The missing piece is deterministic coverage that proves it.

## Desired End State

After this slice:

- a caller cannot accidentally use the classifier-less disposition helper for a
  policy that requires summary classification
- local compact has explicit tests proving it satisfies its artifact-drop and
  marker behavior under the current route law
- governance semantics remain unchanged and separately documented

## Design

### Part A: Harden `apply_history_disposition`

#### Goal

Make misuse of the classifier-less helper impossible or explicit.

#### Required Shape

Use one public production API:

```rust
pub fn apply_history_disposition<F>(
    request: HistoryDispositionRequest,
    is_compaction_summary_message: F,
) -> HistoryDispositionResult
where
    F: Fn(&ResponseItem) -> bool,
```

Do not keep this as a public API:

```rust
pub fn apply_history_disposition(
    request: HistoryDispositionRequest,
) -> HistoryDispositionResult
```

If a classifier-less convenience helper remains, make it `pub(crate)` or
test-only and give it a name that makes its precondition explicit, for example:

```rust
fn apply_history_disposition_without_summary_retention(
    request: HistoryDispositionRequest,
) -> HistoryDispositionResult
```

That helper must only accept policies that do not require summary
classification.

What should not remain:

- a public helper that relies only on `debug_assert!` to protect a semantic
  precondition
- two public production paths where one silently bypasses summary-retention law

#### Caller Migration

Current production callers should converge on the one public classifier-aware
executor:

- `/refresh`: call the public disposition function with the existing local
  summary classifier, even though the current route uses `KeepAll`
- `/prune`: call the same public function with the actual summary classifier
- remote compact shaping: use the same function or a crate-internal helper that
  still requires the classifier when summary retention is relevant
- direct tests that currently call a classifier-less helper should move to the
  explicit internal convenience helper or the public classifier-aware executor

The public export surface in `lib.rs` should reflect this convergence.

#### Expected Touched Surface

- `/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/artifact_codecs.rs`
- `/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/lib.rs`
- any direct tests that exercise the helper surface

#### Validation

- `cargo test -p codex-context-maintenance-policy`
- `cargo test -p codex-core context_maintenance --lib`
- `cargo test -p codex-core compact::tests --lib`
- `cargo check -p codex-core`
- add or update tests proving:
  - summary-retaining disposition uses the classifier-aware path
  - the public path cannot silently accept summary-retaining policy without a
    classifier
  - old public imports of a classifier-less path no longer survive

### Part B: Add Local Compact Disposition Invariants

#### Goal

Lock the actual local compact output against the declared route law without
rewriting local compact to use the shared disposition helper.

#### Required Invariants

At minimum, add focused tests that prove:

1. local intra-turn compact does not retain prior `ContinuationBridge` in the
   replacement history
2. local turn-boundary compact does not retain prior `ThreadMemory` or
   `ContinuationBridge` when the route says they should be dropped
3. local compact strips legacy compaction markers for the active `LocalPure`
   compact routes
4. local compact behavior matches the current context-injection and artifact
   replacement expectations for:
   - intra-turn compact
   - turn-boundary compact

These tests must distinguish:

- prior stale artifacts that must be absent from final replacement history
- newly generated authoritative artifacts that may be present because the route
  requests them

For example:

- intra-turn local compact must drop the old bridge payload, but a newly
  generated continuation bridge may be present
- turn-boundary local compact must drop the old thread memory and old bridge
  payloads, but a newly generated thread memory artifact may be present

Assert by payload identity or tagged payload instance, not only by artifact
kind.

These tests should be framed as invariant checks on final replacement history
against the active plan, not as restatements of the whole route matrix.

#### Allowed Helper Extraction

If direct testing of the current deterministic local assembly path is awkward,
it is acceptable to extract one narrow internal helper from
`codex-rs/core/src/compact.rs` that assembles the local replacement history.

That helper should stay deterministic and limited to the current local compact
assembly sequence. It is not a rewrite to use the shared disposition helper.

#### Expected Touched Surface

- `/home/rose/work/codex/fork/codex-rs/core/src/compact.rs`
- `/home/rose/work/codex/fork/codex-rs/core/src/compact_tests.rs`
- possibly narrow supporting helpers if existing fixtures need reuse

#### Validation

- `cargo test -p codex-core compact::tests --lib`
- if helper layout makes it cleaner, also run the narrow local-compact-specific
  test filter that covers the new invariants

#### Test Framing Rules

Keep these tests plan-derived where practical:

- obtain the active local compact runtime or policy plan for the route under
  test
- assert that final replacement history satisfies that plan's disposition and
  placement contract

Do not hardcode broad duplicate route doctrine in core tests.

Placement assertions should stay minimal:

- intra-turn local compact:
  - initial context is inserted before the last real user or summary
  - the new continuation bridge, when present, is inserted before the final
    summary
- turn-boundary local compact:
  - no initial context is injected
  - the new thread memory artifact, when present, is inserted before the final
    summary

Do not use a blanket "summary is last" assertion when the fixture includes
ghost snapshots. If ghost snapshots are present, account for their current
carry-forward behavior explicitly.

## Governance Rule

This slice must not change the current semantics of
`ThreadMemoryGovernance::Disabled`.

That question remains deferred.

For the current implemented meaning, see:

- [compaction-policy-thread-memory-current-semantics.md](/home/rose/work/codex/fork/docs/fork/compaction-policy-thread-memory-current-semantics.md)

In particular, this slice must not:

- start dropping existing `ThreadMemory` under governance-off
- change thread-memory-gated retention
- rename governance variants

## Test Strategy

Keep the layers separate:

- policy crate tests:
  - helper contract hardening
  - misuse prevention around summary-retention policy
- core tests:
  - final replacement-history invariants for local compact
  - prior artifact absence vs new authoritative artifact placement
  - local marker stripping for the active `LocalPure` routes only

Do not add broad duplicate route-matrix doctrine to core as part of this slice.

## Success Criteria

This slice is successful when:

1. the disposition helper surface no longer permits silent summary-policy misuse
2. local compact has explicit invariant tests covering artifact-drop and marker
   behavior under the current route law
3. no route semantics change
4. governance semantics remain unchanged

## Out Of Scope Follow-Ups

These can happen later if still wanted, but not in this slice:

- clarifying or renaming `ThreadMemoryGovernance::Disabled`
- reducing remaining core route-doctrine tests more broadly
- any additional public API narrowing unrelated to the disposition helper

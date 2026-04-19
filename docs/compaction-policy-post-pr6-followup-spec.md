# Compaction Policy Post-PR6 Follow-Up Spec

## Purpose

This document defines the next hardening family after PR 6.

The earlier extraction and hardening sequence already completed:

- crate extraction
- timing law activation
- governance decoupling
- executable artifact requests
- unified disposition improvements
- retention directives
- cleanup of the bridge supplemental block, export surface, and duplicate text
  helper ownership

This follow-up spec exists because the remaining high-value issues are now
clearer:

- source selection before artifact generation is still under-specified
- history disposition is still partly selected in core
- some protocol-shaped helpers still live outside `codex-protocol`

This spec should be read with:

- [docs/compaction-policy-2nd-audit-review.md](compaction-policy-2nd-audit-review.md)
- [docs/gptpro 2nd audit.md](gptpro%202nd%20audit.md)
- [docs/compaction-policy-hardening-plan.md](compaction-policy-hardening-plan.md)
- [docs/compaction-policy-hardening-implementation-spec.md](compaction-policy-hardening-implementation-spec.md)

## Executive Read

The context-maintenance module is already materially cleaner than the vanilla
0.121 baseline.

The next work should not reopen architecture broadly. It should harden the last
remaining semantic seams that still matter for:

- upstream-ingest stability
- law ownership
- artifact lifetime robustness
- future extensibility without re-entangling `codex-core`

## Primary Goal

By the end of this follow-up family:

- policy-owned artifact lifetimes should be reflected in model-input source
  selection, not just final replacement history
- more history-disposition law should be encoded in the policy plan instead of
  caller-local booleans and predicates
- protocol-shaped helpers should live in `codex-protocol`, not in the policy
  crate

## Family Invariant

Across PR A and PR B, the family should make one rule more executable:

- turn-scoped artifacts are excluded from artifact generation and from final
  carry-forward unless a route explicitly opts in

That invariant already exists implicitly in the route/lifetime doctrine. The
follow-up family should make it hold more uniformly at both:

- model-input source selection time
- final history-disposition time

## Non-Goals

This family should not:

- redesign compaction UX
- reopen TUI/app-server work
- merge observability/E-witness work into the same effort
- build a generic framework for arbitrary future policy engines
- change governance-off semantics unless we explicitly decide that product
  meaning

## Current High-Value Gaps

### 1. Source Selection Before Artifact Generation

Current problem:

- `ThreadMemory` and `ContinuationBridge` can still be generated from prompt
  history that includes stale policy artifacts, especially when no prior
  `ThreadMemory` exists yet

Why it matters:

- `TurnScoped` and durable artifact law is not fully executable at model-input
  time
- artifacts can influence their successors before final-history disposition
  drops them

### 2. History Disposition Is Still Partly Selected In Core

Current problem:

- core still chooses some disposition behavior directly:
  - superseded-artifact pruning
  - prune summary retention
  - remote compact output keep/drop policy

Why it matters:

- this leaves a second history-law surface in `codex-core`

### 3. Protocol Utilities Are Still In The Wrong Layer

Current problem:

- `content_items_to_text` and response-item pairing semantics are still not
  owned by `codex-protocol`

Why it matters:

- these are protocol concerns, not context-maintenance concerns
- keeping them out of protocol increases future upstream-ingest friction

## Recommended PR Sequence

### PR A: Artifact Source-Selection Hardening

#### Goal

Make artifact source selection policy-owned and lifetime-aware at model-input
time.

#### Required Outcome

Add policy-owned source-selection helpers for:

- `ThreadMemory`
- `ContinuationBridge`

Examples of acceptable shape:

```rust
pub fn select_thread_memory_source(
    input: &[ResponseItem],
    is_compaction_summary: impl Fn(&ResponseItem) -> bool,
) -> ThreadMemorySourceSelection

pub enum PriorBridgeVisibility {
    Exclude,
    Allow,
}

pub struct ContinuationBridgeSourceSelectionInput<'a> {
    pub input: &'a [ResponseItem],
    pub prior_bridge_visibility: PriorBridgeVisibility,
}

pub fn select_continuation_bridge_source(
    input: ContinuationBridgeSourceSelectionInput<'_>,
) -> Vec<ResponseItem>
```

Exact names may vary. The important part is that continuation-bridge source
selection should use a narrow request-shaped seam rather than a bare `Vec` if
route-sensitive allowances or classifier inputs are needed.

The rules should become:

- prior `ContinuationBridge` is not visible to generation of the next bridge
  unless a route explicitly allows it
- `PruneManifest` is not visible to new artifact generation unless explicitly
  intended
- `GhostSnapshot` and similar synthetic items are not leaked into artifact
  generation by default
- `ThreadMemory` source filtering no longer depends on a raw summary-prefix
  string; use a classifier callback or similarly narrow adapter input instead

#### Expected Touched Surface

- `codex-rs/context-maintenance-policy/src/thread_memory.rs`
- `codex-rs/context-maintenance-policy/src/continuation_bridge/`
- `codex-rs/core/src/governance/thread_memory.rs`
- `codex-rs/core/src/continuation_bridge.rs`

#### Validation

- policy crate tests for:
  - first thread-memory generation after a prior bridge
  - first thread-memory generation after a prune manifest
  - bridge generation after a prior bridge
  - compaction-summary filtering through a classifier, not a raw string
- `cargo test -p codex-context-maintenance-policy`
- targeted `codex-core` thread-memory and continuation-bridge tests

#### Behavioral Delta

This PR is expected to change model input for maintenance artifacts.

That is acceptable and intentional.

### PR B: History-Plan Completion

#### Goal

Move more history-disposition choices into policy-owned plan data.

#### Required Outcome

The policy plan should own more than:

- `drop_prior_artifact_kinds`
- marker policy
- retention

It should also own the remaining high-value disposition choices, especially:

- whether superseded artifacts are pruned
- summary disposition policy
- the default remote compact output keep/drop law
- prune-manifest accounting inputs, so manifest counts follow the same
  plan-driven disposition result instead of a separate core-side convention

Recommended shape:

- introduce a policy-owned `HistoryDispositionPolicy` or equivalent nested plan
  struct
- remove the caller-supplied `prune_superseded_artifacts` bool from the core
  adapter
- keep classification callbacks in core where they rely on `event_mapping`, but
  make policy own the disposition decision itself

#### Expected Touched Surface

- `codex-rs/context-maintenance-policy/src/contracts.rs`
- `codex-rs/context-maintenance-policy/src/artifact_codecs.rs`
- `codex-rs/context-maintenance-policy/src/history_shape.rs`
- `codex-rs/core/src/context_maintenance_runtime.rs`
- `codex-rs/core/src/context_maintenance.rs`
- `codex-rs/core/src/compact_remote.rs`

#### Validation

- policy crate tests for the new disposition-plan shape
- core route tests for refresh/prune/remote compact behavior using the new
  plan-driven path
- targeted tests for summary retention behavior
- targeted tests showing prune-manifest counts/stats are derived from the same
  disposition result that drives what survives

#### Behavioral Delta

Some deterministic post-processing behavior may move or simplify, but this PR
should not change route semantics intentionally beyond making declared law
uniformly enforced.

### PR C: Protocol Utility Relocation

#### Goal

Move protocol-shaped helpers into `codex-protocol`.

#### Required Outcome

At minimum move:

- `content_items_to_text`
- response-item call/output pairing helpers currently embedded in
  `thread_memory` trimming

Recommended outcome:

- `codex-protocol::models` owns the canonical `ContentItem` text-extraction
  helper
- `codex-protocol::models` owns response-item pair-identity logic
- `context-maintenance-policy` and `codex-core` both consume those helpers from
  protocol

If protocol churn grows more than expected, it is acceptable to split this into:

- PR C1: `content_items_to_text`
- PR C2: response-item call/output pairing helpers

#### Expected Touched Surface

- `codex-rs/protocol/src/models.rs`
- `codex-rs/context-maintenance-policy/src/artifact_codecs.rs`
- `codex-rs/context-maintenance-policy/src/thread_memory.rs`
- all core callers that currently get the helper through `codex-core` or the
  policy crate

#### Validation

- protocol tests for both helpers
- compatibility tests showing no text-conversion drift
- policy crate tests still green
- targeted `codex-core` tests still green

#### Behavioral Delta

No intended behavior change.

This is ownership correction.

### PR D: Explicit Semantics And Cleanup

#### Goal

Resolve remaining ambiguous or cleanup-grade issues after PR A-C land.

#### Candidate Scope

1. clarify governance-off semantics
2. reduce duplicate doctrine in tests
3. narrow any remaining over-broad policy exports
4. clean up duplicated unsupported-route wording

#### Required Discipline

Do not start PR D until PR A-C have landed, because several of these issues may
shrink or disappear after the earlier work.

#### Governance Rule

If `ThreadMemoryGovernance::Disabled` is revisited, do it as an explicit
semantics decision:

- either “do not generate new thread memory, but honor existing”
- or “drop and ignore existing thread memory too”

Do not change this behavior accidentally as a side effect of unrelated cleanup.

## Test Strategy

Keep test layers separate:

- policy crate tests:
  - route doctrine
  - deterministic source/disposition helpers
  - artifact-family exact-output tests
- core tests:
  - adapter mapping
  - production-flow tests using the high-level runtime path
- protocol tests:
  - generic content-item and response-item helpers

Avoid keeping policy exports public solely for the sake of core-side low-level
tests.

## Success Criteria

This follow-up family is complete when:

- artifact lifetime semantics are reflected in both source selection and final
  history
- turn-scoped artifacts are excluded from generation and carry-forward unless a
  route explicitly opts in
- history disposition is materially more plan-driven than caller-driven
- protocol utilities no longer live in the policy crate
- no significant new law surfaces are left in `codex-core` for
  context-maintenance behavior

## Bottom Line

The next work should stay narrow but still be ambitious where it adds long-term
robustness.

The right boundary goal is no longer “extract the policy crate.” That part is
done.

The new goal is:

- make the remaining semantics more executable
- reduce the last important upstream-ingest seams
- avoid broad redesign while still fixing the places where the boundary is not
  yet as strong as it should be

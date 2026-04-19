# Compaction Policy Hardening Implementation Spec

## Purpose

This document defines the implementation contract for the full
context-maintenance hardening family that follows the initial
`codex-context-maintenance-policy` extraction.

The goal is to review the hardening sequence once, then execute it as a series
of small PRs without needing to rediscover the intended direction each time.

This spec is the implementation companion to:

- [docs/compaction-policy-hardening-plan.md](compaction-policy-hardening-plan.md)
- [docs/compaction-policy-hardening-pr6-cleanup-implementation-spec.md](compaction-policy-hardening-pr6-cleanup-implementation-spec.md)
- [docs/compaction-policy-2nd-audit-review.md](compaction-policy-2nd-audit-review.md)
- [docs/compaction-policy-post-pr6-followup-spec.md](compaction-policy-post-pr6-followup-spec.md)
- [docs/compaction-policy-crate-extraction-plan.md](compaction-policy-crate-extraction-plan.md)
- [docs/compaction-policy-route-matrix.md](compaction-policy-route-matrix.md)
- [docs/compaction policy gptpro audit.md](compaction%20policy%20gptpro%20audit.md)

## Interpretation

The initial extraction already succeeded:

- route planning moved into the policy crate
- artifact families moved into the policy crate
- live timing law was activated

This follow-up family is not a redesign.

It is a boundary hardening pass that makes the current policy surface more
executable and reduces the remaining hidden-law surfaces in `codex-core`.

## Primary Goal

By the end of this family:

- the policy crate should no longer depend on config/TOML schema types
- runtime plans should preserve full artifact semantics rather than collapsing
  them to kind-only booleans
- artifact failure and carry/drop behavior should be encoded in plan execution
  rather than hidden in hot core files
- timing classification should be an explicit runtime input rather than derived
  from injection placement
- raw-retention behavior should be policy-directed rather than core-owned

## Non-Goals

This family should not:

- redesign TUI, app-server, or config UX
- reopen the extraction broadly
- absorb sub-agent observability/E-witness work into the same effort
- create a generic policy framework outside the current context-maintenance
  scope
- rewrite upstream/vanilla compaction summary machinery into the artifact model

## Current Boundary Problems To Fix

The hardening family exists because the current merged state still has these
meaningful leaks:

1. the policy crate imports `codex_config::config_toml::GovernancePathVariant`
2. `ArtifactLifetime` is discarded by the core runtime adapter
3. required vs best-effort artifact behavior still lives in core call sites
4. artifact dropping and legacy compaction-marker handling do not run through a
   single executor surface
5. timing is still partially inferred from `InitialContextInjection`
6. raw-retention behavior still depends on core-owned constants and local
   conditionals

This family should remove those leaks in a narrow, staged way.

## PR Family Overview

The recommended sequence remains:

1. Governance decoupling
2. Executable artifact requests
3. Unified disposition
4. Timing/injection decoupling
5. Retention directives
6. Optional cleanup

Each PR should stay narrow and only solve its own leak surface.

## PR 1: Governance Decoupling

### Goal

Replace config/TOML governance types at the policy boundary with a policy-owned
governance axis.

### Required Outcome

The policy crate must stop importing:

- `codex_config::config_toml::GovernancePathVariant`

It must also stop depending on the `codex-config` crate for this purpose. This
PR is not complete if the import disappears but the Cargo edge remains.

Instead, the policy contract should accept a policy-owned input type, using the
narrowest axis that actually changes route behavior.

Examples of acceptable direction:

- `ThreadMemoryGovernance`
- `GovernancePolicyMode`

The key rule is:

- do not introduce broader distinctions than the route matrix currently needs
- prefer the narrowest route-relevant policy enum now, unless the route matrix
  already needs something broader

Core adapter rule:

- mapping from runtime config into the policy enum must fail closed for unknown
  future runtime/config variants

### Expected Touched Surface

- [codex-rs/context-maintenance-policy/src/contracts.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/contracts.rs)
- [codex-rs/context-maintenance-policy/src/route_matrix.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/route_matrix.rs)
- [codex-rs/context-maintenance-policy/src/tests.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/tests.rs)
- [codex-rs/core/src/context_maintenance_runtime.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance_runtime.rs)

### Scope Guard

Do not mix into PR 1:

- artifact lifetime/requiredness changes
- timing classifier changes
- marker/disposition changes

### Validation

- policy crate contract tests updated
- core adapter translation tests updated
- `cargo test -p codex-context-maintenance-policy`
- targeted `codex-core` runtime-plan tests if needed

### Behavioral Delta

This PR should be boundary-only:

- no intended route-matrix change
- no intended visible runtime behavior change

## PR 2: Executable Artifact Requests

### Goal

Preserve the full policy-owned artifact request surface through the runtime
adapter instead of collapsing it to kind-only booleans.

### Required Outcome

The runtime adapter should preserve, at minimum:

- `ArtifactKind`
- `ArtifactLifetime`
- `ArtifactRequiredness`

The preferred shape is to preserve the full policy-owned `ArtifactRequest`
surface into runtime rather than reconstructing a runtime-local equivalent.

Recommended addition:

```rust
pub enum ArtifactRequiredness {
    Required,
    BestEffort,
}
```

Expected initial requiredness:

- `ThreadMemory` -> `Required`
- `ContinuationBridge` -> `BestEffort`
- `PruneManifest` -> `Required`

The runtime layer should stop interpreting requested artifacts through
`requests_artifact(kind)` as the only executable contract.

Clarification:

- this PR preserves lifetime and requiredness into runtime
- it does not yet fully operationalize carry/drop law; that belongs to PR 3 and
  PR 5

### Expected Touched Surface

- [codex-rs/context-maintenance-policy/src/contracts.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/contracts.rs)
- [codex-rs/context-maintenance-policy/src/route_matrix.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/route_matrix.rs)
- [codex-rs/context-maintenance-policy/src/tests.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/tests.rs)
- [codex-rs/core/src/context_maintenance_runtime.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance_runtime.rs)
- [codex-rs/core/src/compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)
- [codex-rs/core/src/compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs)
- [codex-rs/core/src/context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)

### Scope Guard

Do not mix into PR 2:

- marker/disposition unification
- timing-classifier redesign
- retention directive movement

### Validation

- policy route-plan tests updated for requiredness/lifetime
- core adapter/runtime tests updated
- live runtime behavior tests cover required vs best-effort artifact generation
- `cargo test -p codex-context-maintenance-policy`
- targeted `codex-core` tests for compact/refresh/prune

Validation must include failure-path coverage proving that `Required` vs
`BestEffort` behavior is enforced through runtime execution, not merely stored
in the plan.

### Behavioral Delta

This PR should be mostly contract-executable:

- minimal visible runtime behavior change
- the main outcome is preserving and honoring richer artifact semantics

## PR 3: Unified Disposition

### Goal

Run artifact carry/drop law and legacy compaction-marker law through one
deterministic executor surface.

### Required Outcome

One executor helper should own:

- superseded tagged artifact pruning
- explicit prior-artifact removal by kind
- legacy `ResponseItem::Compaction` preservation or stripping

The important constraint is:

- core should consume a policy disposition plan, not a collapsed bool such as
  `preserve_marker`
- the full legacy marker policy enum must survive to the executor boundary

Use one canonical mechanism for marker handling inside the disposition
executor. Do not let marker behavior become parallel law surfaces split between
drop lists and unrelated booleans.

This PR should remove the current split where some routes get the effect by
incidental history rebuilding and others iterate `drop_prior_artifact_kinds()`
locally.

Boundary note:

- if summary-message retention remains core-owned in this PR, that boundary
  should stay explicit rather than being partially absorbed by accident

### Expected Touched Surface

- [codex-rs/context-maintenance-policy/src/contracts.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/contracts.rs)
- [codex-rs/context-maintenance-policy/src/artifact_codecs.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/artifact_codecs.rs)
- [codex-rs/context-maintenance-policy/src/history_shape.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/history_shape.rs)
- [codex-rs/core/src/context_maintenance_runtime.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance_runtime.rs)
- [codex-rs/core/src/compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)
- [codex-rs/core/src/compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs)
- [codex-rs/core/src/context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)

### Scope Guard

Do not mix into PR 3:

- runtime timing classifier changes
- raw-retention directive movement

### Validation

- policy crate disposition helper tests
- core tests for refresh/prune/compact carry-drop behavior
- tests proving legacy compaction markers follow the policy enum, not a reduced
  bool

Validation must include route-level fixtures across:

- local compact
- remote compact
- refresh
- prune

### Behavioral Delta

This PR may intentionally change outcomes where marker/disposition policy was
previously declared but not enforced uniformly.

## PR 4: Timing/Injection Decoupling

### Goal

Make runtime timing classification an explicit input and keep context injection
as policy output.

### Required Outcome

The law should become:

- runtime phase classifies timing
- policy receives timing
- policy returns context-injection placement

That means core should stop deriving timing from:

- `InitialContextInjection`

Recommended shape:

- introduce an explicit runtime timing-classifier type
- keep timing classification separate from injection placement at the adapter
  boundary

Classifier rule:

- timing should be classified once at the call boundary and then passed through
- downstream code should not re-infer timing from helper choices or injection
  state

This PR should also fix stale semantic narrowing where the core name still says
“last user message” while the policy law is “last real user or summary.”

### Expected Touched Surface

- [codex-rs/core/src/context_maintenance_runtime.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance_runtime.rs)
- [codex-rs/core/src/compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)
- [codex-rs/core/src/compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs)
- [codex-rs/core/src/codex.rs](/home/rose/work/codex/fork/codex-rs/core/src/codex.rs)
- [codex-rs/core/src/compaction_policy_matrix_tests.rs](/home/rose/work/codex/fork/codex-rs/core/src/compaction_policy_matrix_tests.rs)

### Scope Guard

Do not mix into PR 4:

- raw-retention directive movement
- continuation-bridge supplemental-block ownership

### Validation

- policy route-law tests remain green
- core timing-classifier tests added or updated
- live runtime tests for pre-turn vs intra-turn compact behavior

### Behavioral Delta

This PR is a law-surface cleanup:

- it should change ownership of timing classification
- it should not intentionally change the route matrix itself

## PR 5: Retention Directives

### Goal

Move raw-retention behavior out of core-owned constants and local conditionals
into policy-directed route data.

### Required Outcome

Retention behavior should become plan-driven, including at least:

- whether raw conversation retention applies for a route
- the retention window size
- whether retention depends on a durable artifact such as `ThreadMemory`

Design choice for this family:

- retention is a route-level directive in the maintenance plan
- it is not attached to an artifact request

If retention depends on durable artifact presence such as `ThreadMemory`, that
dependency should be encoded in policy-owned route data, not rediscovered in
core by checking artifact kinds again.

This is not optional cleanup. It is still policy law because it changes
visible post-compaction behavior.

### Expected Touched Surface

- [codex-rs/context-maintenance-policy/src/contracts.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/contracts.rs)
- [codex-rs/context-maintenance-policy/src/route_matrix.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/route_matrix.rs)
- [codex-rs/context-maintenance-policy/src/history_shape.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/history_shape.rs)
- [codex-rs/core/src/context_maintenance_runtime.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance_runtime.rs)
- [codex-rs/core/src/compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)
- [codex-rs/core/src/compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs)
- [codex-rs/core/src/context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)

### Scope Guard

Do not mix into PR 5:

- public API narrowing
- unrelated helper deduplication

### Validation

- policy retention-directive tests
- core history-shaping/runtime tests for retained raw tail behavior
- targeted tests for routes that keep or do not keep raw conversation history

Validation should cover:

- local compact
- remote compact
- refresh/prune

Review expectation:

- this PR is the one most likely to cause visible retained-tail fixture changes
- snapshot/fixture churn here is expected if the policy-owned directive changes
  retained-tail outcomes

### Behavioral Delta

This PR may intentionally change retained-tail behavior where current core-owned
conditionals diverge from the desired policy-owned rule.

## PR 6: Optional Cleanup

### Goal

Finish low-risk cleanup after the main law surfaces are sealed.

### Acceptable Content

- narrow public exports from the policy crate
- move continuation-bridge supplemental subagent block ownership if still split
- remove duplicate seam utilities

Keep these as separate cleanup buckets. Do not force them into one patch if one
of them grows unexpectedly.

### Scope Guard

Do not use PR 6 to sneak in:

- new timing-law behavior
- new artifact families
- observability/E-witness work

### Validation

- focused crate tests only for the touched cleanup area

## Testing Layer Rules

Each PR in this family should state clearly which test layer it updates:

- policy crate contract tests
- core adapter/runtime tests
- live behavior fixtures
- historical/transitional tests, if any remain and still serve a clear purpose

Avoid mixing these levels implicitly. The point of the hardening family is to
reduce hidden law, not move it into confusing test layering.

## Success Criteria

This family is successful when:

- the policy crate no longer depends on config/TOML schema types
- runtime plans preserve artifact lifetime and requiredness
- required vs best-effort behavior is readable from the plan
- artifact and legacy marker disposition run through one path
- timing is no longer inferred from injection placement
- raw-retention behavior is policy-directed rather than core-owned

Cross-cutting constraint:

- [context_maintenance_runtime.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance_runtime.rs)
  remains an adapter that preserves policy semantics; it must not become a
  second hidden law surface that reinterprets them

At that point the context-maintenance boundary should be strong enough to serve
as the reference extraction pattern for the later sub-agent observability work.

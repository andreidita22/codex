# Compaction Policy Hardening Plan

## Purpose

The initial `codex-context-maintenance-policy` extraction is complete and
merged, but the boundary is not fully sealed.

This document defines the next hardening pass:

- keep the existing crate and route law
- avoid reopening the extraction broadly
- tighten the remaining semantic leaks between `codex-core` and the policy
  crate

This is a follow-up to:

- [docs/compaction-policy-crate-extraction-plan.md](compaction-policy-crate-extraction-plan.md)
- [docs/compaction-policy-hardening-implementation-spec.md](compaction-policy-hardening-implementation-spec.md)
- [docs/compaction-policy-hardening-pr6-cleanup-implementation-spec.md](compaction-policy-hardening-pr6-cleanup-implementation-spec.md)
- [docs/compaction-policy-2nd-audit-review.md](compaction-policy-2nd-audit-review.md)
- [docs/compaction-policy-post-pr6-followup-spec.md](compaction-policy-post-pr6-followup-spec.md)
- [docs/compaction-policy-route-matrix.md](compaction-policy-route-matrix.md)
- [docs/compaction policy gptpro audit.md](compaction%20policy%20gptpro%20audit.md)

## Executive Read

The extraction was worth doing and should stand.

The right next move is not a redesign. It is a short hardening program that
makes the policy contract more executable and removes the largest remaining
hidden-law surfaces.

The most important remaining gaps are:

1. the policy crate still imports a TOML/config enum for governance
2. `ArtifactLifetime` is declared in policy but discarded in core
3. artifact requiredness is still encoded in core call sites
4. artifact/marker disposition is not applied through one executor surface
5. timing is still partially inferred from injection placement in core
6. raw-retention law is still driven by core-owned constants rather than a
   policy-owned directive

## Hardening Goals

The follow-up work should make the following true:

- the policy crate depends on policy-owned enums, not config/TOML schema
- core preserves full artifact request semantics from the policy plan
- artifact failure behavior is encoded in the plan, not rediscovered in core
- carry/drop and legacy compaction-marker law runs through one deterministic
  executor path
- runtime timing classification is an explicit input, while context injection is
  an output chosen by policy

## Scope Discipline

This hardening pass should not:

- redesign TUI or app-server surfaces
- change the existence of `codex-context-maintenance-policy`
- merge unrelated sub-agent observability work into the same effort
- expand into a generic "policy framework"

It should stay focused on sealing the context-maintenance boundary we already
created.

## Priority Order

### 1. Policy-owned governance axis

Replace the policy crate's dependency on:

- `codex_config::config_toml::GovernancePathVariant`

with a policy-owned input enum, for example:

- `GovernancePolicyMode`

or, if route semantics only care about thread-memory eligibility:

- `ThreadMemoryGovernance`

`codex-core` should translate runtime config into this policy enum in the
adapter layer.

Rule:

- use the narrowest policy-owned governance axis that actually affects route
  planning
- do not introduce a broad governance enum unless the route matrix really needs
  those distinctions

Why first:

- this is the cleanest low-risk boundary fix
- it removes a direct config-schema leak from the policy crate
- it improves future ingestability immediately

### 2. Preserve full artifact request semantics into core

Do not flatten policy `ArtifactRequest` into kind-only runtime booleans.

The runtime adapter should preserve the full policy-owned artifact request
surface, including at least:

- `ArtifactKind`
- `ArtifactLifetime`
- `ArtifactRequiredness`

Recommended addition:

```rust
pub enum ArtifactRequiredness {
    Required,
    BestEffort,
}
```

Expected initial mapping:

- `ThreadMemory` -> `Required`
- `ContinuationBridge` -> `BestEffort`
- `PruneManifest` -> `Required`

Why second:

- this turns the current mostly-documentary artifact contract into executable
  law
- it removes failure semantics from hidden core conventions

Non-goal:

- this does not try to absorb upstream/vanilla compaction-summary machinery into
  the same artifact model

### 3. Unify artifact disposition and legacy marker handling

Introduce one deterministic executor helper for prior-artifact and marker law.

It should own:

- superseded tagged artifact pruning
- explicit prior artifact removal by kind
- legacy `ResponseItem::Compaction` stripping or preservation

The important outcome is not the exact helper name. It is having one executor
surface for a full policy disposition plan, not a reduced bool or ad hoc flags.

That executor surface should preserve:

- `drop_prior_artifact_kinds`
- `LegacyCompactionMarkerPolicy`

Why third:

- current behavior is correct in many paths by incidental rebuilding/filtering
- that is acceptable for now, but it is still split law
- unifying this lowers future regression risk when new artifact families are
  added

### 4. Decouple timing classification from context injection

Today core still derives timing from:

- `InitialContextInjection`

That inversion should be removed.

The intended law is:

- runtime classifies timing from the turn phase
- policy receives timing as input
- policy returns context-injection placement as output

Recommended shape:

- introduce an explicit runtime timing classifier type
- keep timing classification and injection placement as separate concepts at the
  adapter boundary

This keeps timing and injection from becoming two hidden control surfaces for
the same behavior.

Why this is top-tier, not polish:

- future timing classes will be awkward while timing is derived from injection
- the current arrangement leaves a second implicit semantic axis in core

### 5. Move raw-retention directives into policy

Raw-retention law is still policy-shaped behavior, but it is currently driven by
core-owned constants and local branching.

This should become policy-owned directive data rather than incidental core law.

Examples:

- whether raw conversation retention applies for a route
- the retention window size
- whether retention is conditioned on a durable artifact such as thread memory

Why this is mandatory:

- retention changes visible post-compaction behavior
- it is more than convenience cleanup
- if it remains core-owned, the boundary is still not fully policy-shaped

### 6. Smaller ownership cleanup after the law surfaces are fixed

After the four items above, finish the smaller remaining splits:

- continuation-bridge supplemental subagent block ownership
- duplicate seam utilities such as overlapping text extraction helpers
- overly broad public exports from the policy crate

These matter, but they should follow the main contract hardening rather than
precede it.

## Recommended PR Sequence

The best shape is a short sequence of small PRs rather than one broad pass.

### PR 1: Governance decoupling

Land:

- policy-owned governance enum
- core adapter translation
- policy tests updated accordingly

Do not mix in timing or artifact-shape changes.

Testing focus:

- policy crate contract tests
- core adapter translation tests

### PR 2: Executable artifact requests

Land:

- `ArtifactRequiredness`
- runtime preservation of full artifact requests
- core generation paths switched from `requests_artifact(kind)` to consuming the
  richer runtime plan

Do not mix in marker disposition yet.

Testing focus:

- policy crate plan tests
- core runtime-plan tests
- live runtime behavior tests for required vs best-effort handling

### PR 3: Unified disposition

Land:

- deterministic artifact/marker disposition helper
- refresh/prune/compact paths routed through the same drop/marker logic
- tests updated to assert the same carry/drop law everywhere

Testing focus:

- policy crate disposition helper tests
- core refresh/prune/compact behavior tests
- any historical/transitional tests still needed for old marker behavior

### PR 4: Timing/injection decoupling

Land:

- explicit runtime timing classifier
- remove timing derivation from `InitialContextInjection`
- keep injection as policy output only
- adjust naming if needed so core does not narrow the semantics to
  "last user message" when the law is "last real user or summary"

Testing focus:

- policy route-law tests
- core adapter timing-classifier tests
- live runtime behavior tests for intra-turn vs turn-boundary routing

### PR 5: Retention directives

Land:

- policy-owned raw-retention directive data
- core stops owning route-specific retention-law constants
- runtime uses policy-provided retention behavior rather than local conditionals

Testing focus:

- policy retention-directive tests
- core history-shaping/runtime tests for retained raw tail behavior

### Optional PR 6: Cleanup

Only if still worthwhile after the earlier four:

- narrow the policy crate's public API
- move remaining supplemental asset ownership
- remove duplicate seam helpers

## Success Criteria

This hardening pass is successful when:

- the policy crate no longer imports config/TOML schema types
- runtime plans preserve artifact lifetime and requiredness
- required vs best-effort behavior is readable from the plan, not from scattered
  core branches
- artifact and legacy marker disposition run through one path
- timing is no longer inferred from injection placement
- raw-retention behavior is policy-directed rather than core-owned

At that point the context-maintenance crate boundary is strong enough to use as
the reference pattern for the later sub-agent observability extraction.

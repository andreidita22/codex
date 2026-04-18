# Compaction Policy P4 Timing Law Implementation Spec

## Purpose

This spec defines `p4` of the compaction-policy extraction:

- make `MaintenanceTiming` authoritative in live runtime routing
- switch `/compact`, `/refresh`, and `/prune` from current-behavior preservation
  to the target timing law
- preserve the `p3` ownership boundary while changing semantics deliberately

This step follows:

- [docs/compaction-policy-crate-extraction-plan.md](compaction-policy-crate-extraction-plan.md)
- [docs/compaction-policy-route-matrix.md](compaction-policy-route-matrix.md)
- [docs/compaction-policy-p0-route-matrix-implementation-spec.md](compaction-policy-p0-route-matrix-implementation-spec.md)
- [docs/compaction-policy-p1-scaffold-implementation-spec.md](compaction-policy-p1-scaffold-implementation-spec.md)
- [docs/compaction-policy-p2-history-shaping-implementation-spec.md](compaction-policy-p2-history-shaping-implementation-spec.md)
- [docs/compaction-policy-p3-artifact-codecs-implementation-spec.md](compaction-policy-p3-artifact-codecs-implementation-spec.md)

## Interpretation

`p4` is the first semantic activation step.

`p0` through `p3` were intentionally conservative:

- lock current behavior
- extract policy seams
- move artifact ownership
- avoid live route-law changes

`p4` is where the extracted policy crate starts deciding what the runtime
actually does.

That means `p4` must be explicit about which current behaviors are being
removed, not just which new types are now in use.

## Primary Goal

Replace scattered call-site conditionals with one authoritative timing-aware
policy decision for live maintenance flows.

By the end of `p4`:

- `/compact` should route through explicit `MaintenanceTiming`
- `/refresh` should stop generating `continuation_bridge`
- `/prune` should remain purely deterministic boundary hygiene
- unsupported intra-turn routes should fail closed
- `codex-core` should execute a policy plan, not rediscover timing law locally
- bridge carry-forward/drop rules should be enforced explicitly rather than
  implied by generation choices alone

## Target Semantic Changes

`p4` intentionally changes live behavior in these ways.

### 1. Intra-turn compact stops regenerating durable thread memory

Current preserved behavior already suppresses durable thread-memory regeneration
for the mid-turn injection path in some places, but the law is still implicit.

After `p4`:

- `Compact + IntraTurn + LocalPure`
  - generate `continuation_bridge` only
- `Compact + IntraTurn + RemoteHybrid`
  - generate `continuation_bridge` only
- `Compact + IntraTurn + RemoteVanilla`
  - no fork-owned artifacts

This should be policy-driven rather than inferred from local helper choices.

### 2. Turn-boundary refresh stops generating continuation bridge

This is the main current-vs-target semantic delta.

Current behavior:

- `/refresh` regenerates both `thread_memory` and `continuation_bridge`

Target behavior:

- `Refresh + TurnBoundary + LocalPure`
  - regenerate durable `thread_memory`
  - do not generate `continuation_bridge`
- `Refresh + TurnBoundary + RemoteHybrid`
  - same
- `Refresh + TurnBoundary + RemoteVanilla`
  - unsupported

This should be treated as an intentional behavioral change, not a side effect
of refactoring.

### 3. Intra-turn refresh/prune become explicitly unsupported

Current runtime does not expose them as true route variants, but the law is not
yet fail-closed through the new policy contract.

After `p4`, these must produce typed unsupported-route failures:

- `Refresh + IntraTurn + any engine`
- `Prune + IntraTurn + any engine`

### 4. Bridge lifetime becomes operational rather than descriptive

The policy contract already names:

- `continuation_bridge` as `TurnScoped`
- `thread_memory` as `DurableAcrossTurns`

After `p4`, runtime behavior must actually follow that law:

- turn-boundary flows must not carry forward `continuation_bridge` as durable
  continuity state
- intra-turn flows may generate bridge state, but that bridge is not the next
  turn’s durable canonical memory

This means `p4` must govern both:

- what new artifacts are generated
- which prior artifacts are explicitly dropped or stripped from rebuilt history

## Scope

`p4` should do five things:

1. route live maintenance entrypoints through `MaintenancePlanningRequest`
2. map runtime call sites to explicit `MaintenanceAction` and
   `MaintenanceTiming`
3. execute only the artifacts requested by the returned `MaintenancePolicyPlan`
4. enforce explicit artifact carry-forward/drop policy, especially for
   `continuation_bridge`
5. remove local duplicated timing-law conditionals that are replaced by plan
   execution
6. add tests that prove the target route matrix is now live behavior

`p4` should not:

- redesign the `p1` planning contract
- add new engines
- change TUI/config/app-server surface area
- move runtime execution or persistence out of `codex-core`
- redesign prompt layers or memory-tool behavior

## Current Code Surface To Rewire

### `core/src/compact.rs`

This is the main semantic activation site.

`p4` should stop letting these helpers act as the true source of timing law:

- `InitialContextInjection`
- `should_regenerate_thread_memory(...)`
- local decision branches around thread-memory and bridge generation

The file should still execute compaction, but it should execute a policy plan
rather than infer semantics locally.

### `core/src/compact_remote.rs`

Remote hybrid and remote vanilla compact paths must also consume the policy
plan:

- whether authoritative fork artifacts are generated
- whether initial context is injected
- whether legacy compaction markers are preserved

The remote path should not keep a private timing-law fork after `p4`.

### `core/src/context_maintenance.rs`

This is the main activation site for `/refresh` and `/prune`.

`p4` must change:

- `/refresh` should stop generating `continuation_bridge`
- `/prune` remains manifest-only
- route failure for unsupported combinations must be explicit

### `core/src/compaction_policy_matrix_tests.rs`

The existing `p0` current-state lock tests remain valuable, but they stop being
the runtime truth after `p4`.

`p4` should:

- keep the current-state tests as historical regression context where useful
- add or update runtime-oriented tests so the target matrix is asserted against
  actual live routing behavior

## Recommended Runtime Contract Shape

At runtime, `codex-core` should build a planning request like:

```rust
let request = MaintenancePlanningRequest {
    action,
    timing,
    engine,
    governance_variant,
};
let plan = plan_route(request)?;
```

Then execution should follow `plan`, not re-derive semantics from:

- current helper booleans
- ad hoc engine checks
- action-specific local assumptions

This does not require a giant new executor type in `p4`. A small local adapter
layer in `codex-core` is enough, for example:

```rust
struct RuntimeMaintenancePlan {
    context_injection: ...,
    requested_artifacts: Vec<ArtifactRequest>,
    drop_prior_artifact_kinds: Vec<ArtifactKind>,
    preserve_bridge_in_replacement_history: bool,
    legacy_compaction_marker_policy: ...,
    governance_effects: Vec<GovernanceEffect>,
}
```

Do not flatten `p4` into only booleans like `generate_continuation_bridge`.
That loses the actual lifetime law this phase is supposed to activate.

The important part is that this adapter is derived from the policy crate, not
hand-authored per call site, and that it carries both generation and
carry-forward/drop policy.

### Governance variant in `p4`

`governance_variant` must not remain semantically undefined during live
activation.

For `p4`, its live role is intentionally narrow:

- it may suppress `thread_memory` request generation through the existing
  policy-plan overlay when governance is `Off`
- it does not alter timing classification
- it does not re-enable bridge generation on turn-boundary routes
- it does not change unsupported-route handling

If `governance_variant` is needed for richer timing-law behavior later, that
should be introduced as a new semantic addition after `p4`, not left implicit.

## Mapping Rules To Activate

`p4` should add one small timing classifier/helper in `codex-core` so timing is
decided once and then passed into planning, rather than re-inferred at multiple
call sites.

For example:

```rust
fn maintenance_timing_for_compact(trigger: CompactTrigger) -> MaintenanceTiming;
```

### `/compact`

Manual/pre-turn `/compact` must map to:

- `MaintenanceAction::Compact`
- `MaintenanceTiming::TurnBoundary`

Inline auto-compaction during an active turn must map to:

- `MaintenanceAction::Compact`
- `MaintenanceTiming::IntraTurn`

### `/refresh`

`/refresh` must map to:

- `MaintenanceAction::Refresh`
- `MaintenanceTiming::TurnBoundary`

No bridge generation is allowed once this route is policy-driven.

### `/prune`

`/prune` must map to:

- `MaintenanceAction::Prune`
- `MaintenanceTiming::TurnBoundary`

It remains deterministic and artifact-light:

- no new `thread_memory`
- no new `continuation_bridge`
- prune manifest only

## Policy-Plan Execution Rules

`p4` should make these execution rules explicit.

### Artifact generation

Only generate artifacts present in `plan.requested_artifacts`.

That means:

- do not call thread-memory generation just because governance is enabled if
  the selected route does not request it
- do not call continuation-bridge generation on turn-boundary refresh

### Artifact carry-forward and drop policy

Generation rules are not sufficient by themselves.

`p4` must also explicitly control which prior artifacts survive rebuilt
history.

Required rule:

- turn-boundary routes that do not permit durable bridge carry-forward must
  strip prior `continuation_bridge` artifacts from replacement history

Recommended execution shape:

- derive `drop_prior_artifact_kinds` from the policy plan or runtime adapter
- apply that drop set before reinserting authoritative artifacts
- treat bridge lifetime enforcement as a first-class runtime behavior, not as
  an accident of whether a new bridge was generated

### Context injection

Use `plan.context_injection` as the single source of truth for rebuilt-history
injection policy.

`InitialContextInjection` may remain as a core-side adapter enum if needed for
existing shaping helpers, but it should become a translation target rather than
the law itself.

### Legacy compaction markers

Use `plan.legacy_compaction_marker_policy` rather than per-path special cases.

### Unsupported routes

Unsupported routes must surface as typed planning failures and be converted
into clear runtime errors, not silent fallback behavior.

`p4` should use a stable user-visible runtime message for unsupported routes, so
existing surfaces that can still reach them do not silently no-op. The exact
string can remain a core-side wrapper over `MaintenancePolicyError`, but it
must be:

- explicit about the action/timing/engine combination
- stable enough to test
- clearly distinct from transient generation failures

## Test Strategy

`p4` needs two test layers.

### 1. Policy-crate route tests

These already exist from `p1` and should remain the target semantic lock.

No major redesign is needed here beyond any additions required by final live
adoption.

### 2. Runtime activation tests

Add or update tests in `codex-core` that prove live behavior now follows the
target matrix.

Required assertions:

- intra-turn compact does not regenerate `thread_memory`
- turn-boundary compact does regenerate durable `thread_memory` where the
  target route says it should
- `/refresh` does not generate `continuation_bridge`
- `/prune` emits only prune manifest behavior
- unsupported routes fail closed if constructed

Recommended locations:

- [compact_tests.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_tests.rs)
- [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)
- [compaction_policy_matrix_tests.rs](/home/rose/work/codex/fork/codex-rs/core/src/compaction_policy_matrix_tests.rs)

The transition away from `p0` current-state assertions should be explicit:

- old `/refresh` overlap assertions must not remain mixed with the new live-law
  assertions as if both were normative
- any retained current-state fixtures should be clearly marked as historical
  migration context only
- the green runtime suite after `p4` should assert the target law, not the old
  overlap behavior

## Recommended Commit Shape

Keep `p4` reviewable by implementing it in this order:

1. add a small core-side adapter from `MaintenancePolicyPlan` to runtime flags
2. wire `/refresh` and `/prune` through the adapter
3. wire local compact paths through the adapter
4. wire remote compact paths through the adapter
5. update runtime tests to target the activated law

This keeps the highest-risk behavioral delta, `/refresh`, isolated early.

## Exit Criteria

`p4` is complete when all of the following are true:

1. live `/compact`, `/refresh`, and `/prune` route through explicit
   `MaintenanceAction` + `MaintenanceTiming`
2. no live path still relies on duplicated local timing-law conditionals as the
   true source of semantics
3. `/refresh` no longer generates `continuation_bridge`
4. unsupported intra-turn refresh/prune routes fail closed
5. targeted core tests prove the target matrix is now the actual runtime
   behavior

## Non-Goals

`p4` does not need to:

- collapse every old helper type immediately if it still works as an adapter
- redesign the full final assembly surface
- introduce `p5` cleanup in the same slice

If `p4` needs small transitional adapters in `codex-core`, that is acceptable
as long as:

- the policy crate is now the semantic source of truth
- the adapters are obviously transitional rather than a second law surface

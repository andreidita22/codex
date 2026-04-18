# Compaction Policy P1 Scaffold Implementation Spec

## Purpose

This spec defines `p1` of the compaction-policy extraction:

- create the new fork-owned crate
- establish the typed policy contract
- encode the target route matrix in the new crate
- avoid changing runtime behavior yet

This step follows:

- [docs/compaction-policy-crate-extraction-plan.md](compaction-policy-crate-extraction-plan.md)
- [docs/compaction-policy-route-matrix.md](compaction-policy-route-matrix.md)
- [docs/compaction-policy-p0-route-matrix-implementation-spec.md](compaction-policy-p0-route-matrix-implementation-spec.md)

## Interpretation

`p1` is a **crate scaffold plus typed contract** step.

It is not yet a history-shaping extraction and it is not yet a timing-law
activation step.

The purpose is to create the future home for fork-owned context-maintenance
logic without making `codex-core` depend on unfinished policy behavior.

## Scope

`p1` should do four things:

1. add a new workspace crate:
   - `codex-context-maintenance-policy`
2. define the minimal typed request/plan/result vocabulary for later phases
3. encode the target route matrix as pure crate-local logic
4. add crate-local tests that lock those target semantics

`p1` should not:

- move any runtime compaction logic out of `codex-core`
- replace the `p0` current-state lock tests
- change app-server, TUI, or config behavior
- make `codex-core` route live maintenance through the new crate
- introduce compatibility shims that guess at unfinished behavior

## Why P1 Comes After P0

`p0` locked the current implementation surface in `codex-core`.

That means `p1` can now define the future policy vocabulary without pretending
the current runtime already follows it.

The two layers should remain explicit:

- `p0`
  - current observed behavior in `codex-core`
- `p1`
  - target policy contract in the new crate

This prevents the scaffold step from accidentally becoming a runtime
convergence step.

## Architectural Objective

After `p1`, the repo should have a dedicated crate that can answer questions
like:

- what route is this action/timing/engine combination?
- how does `governance_variant` refine that route?
- which artifacts are allowed or forbidden?
- what context injection policy applies?
- what artifact lifetime should each generated artifact have?

But it should **not** yet be responsible for:

- extracting artifacts from history
- shaping replacement history
- parsing or encoding artifact payloads
- calling models
- mutating session state

Those are later phases.

## Route-Law Interpretation

`p1` should treat policy evaluation as a two-step process:

1. route selection
   - driven by:
     - `MaintenanceAction`
     - `MaintenanceTiming`
     - policy-owned engine enum
2. governance overlay
   - driven by:
     - `GovernancePathVariant`

This matters because `governance_variant` is part of the policy contract, but
it is not a fourth axis in the route matrix. The route matrix answers:

- which route exists?
- which artifact families are eligible in principle?

The governance overlay answers:

- within a supported route, which fork-owned artifacts are actually requested?

Example:

- `Compact + TurnBoundary + LocalPure`
  - route law says `thread_memory` is eligible
- `GovernancePathVariant::Off`
  - overlay may suppress that artifact request

So `p1` should not model the route matrix as if it alone determines the final
plan.

## Proposed Crate Layout

New crate path:

- `codex-rs/context-maintenance-policy`

Suggested initial file layout:

- `codex-rs/context-maintenance-policy/Cargo.toml`
- `codex-rs/context-maintenance-policy/src/lib.rs`
- `codex-rs/context-maintenance-policy/src/contracts.rs`
- `codex-rs/context-maintenance-policy/src/route_matrix.rs`
- `codex-rs/context-maintenance-policy/src/tests.rs`

This keeps the first slice small and avoids overdesigning internal modules
before real logic starts moving.

## Dependency Discipline

The new crate should stay as light as possible.

Preferred dependencies:

- `codex-config`
  - for `GovernancePathVariant`
- `serde`
  - only if deriving serialization is useful for test fixtures or future API
    surfaces

Do not depend on:

- `codex-core`
- `codex-tui`
- `codex-app-server`

The new crate must remain below the kernel layer, not above it.

For `p1`, prefer a crate-local policy-owned engine enum instead of reusing the
existing config enum directly. Translation from config/runtime enums can be
added later when `codex-core` starts integrating with the new crate.

## Deliverables

### 1. Workspace wiring

Add the crate to:

- `codex-rs/Cargo.toml`
  - `[workspace].members`
  - `[workspace.dependencies]`

This should make the crate available to later phases without yet forcing
adoption in `codex-core`.

### 2. Minimal typed contracts

Define the first-class types already promised by the extraction plan.

At minimum:

```rust
pub enum MaintenanceAction {
    Compact,
    Refresh,
    Prune,
}

pub enum MaintenanceTiming {
    TurnBoundary,
    IntraTurn,
}

pub enum ContextInjectionPolicy {
    None,
    BeforeLastRealUserOrSummary,
}

pub enum ArtifactLifetime {
    TurnScoped,
    DurableAcrossTurns,
    MarkerOnly,
}

pub enum PolicyEngine {
    RemoteVanilla,
    RemoteHybrid,
    LocalPure,
}

pub enum ArtifactKind {
    ThreadMemory,
    ContinuationBridge,
    PruneManifest,
    CompactionMarker,
}
```

And a planning-only input:

```rust
pub struct MaintenancePlanningRequest {
    pub action: MaintenanceAction,
    pub timing: MaintenanceTiming,
    pub engine: PolicyEngine,
    pub governance_variant: GovernancePathVariant,
}
```

And a first planning result:

```rust
pub struct ArtifactRequest {
    pub kind: ArtifactKind,
    pub lifetime: ArtifactLifetime,
}

pub struct MaintenancePolicyPlan {
    pub context_injection: ContextInjectionPolicy,
    pub requested_artifacts: Vec<ArtifactRequest>,
    pub preserve_legacy_compaction_marker: bool,
    pub governance_overlay_applied: bool,
}

pub enum MaintenancePolicyError {
    UnsupportedRoute {
        action: MaintenanceAction,
        timing: MaintenanceTiming,
        engine: PolicyEngine,
    },
}
```

The exact field names may vary, but the core requirement is:

- lifetime must be attached to requested/generated artifacts as executable data
- unsupported routes must fail closed via a typed error
- `p1` remains a planning-only seam
- richer runtime request types are deferred to `p2/p3`

Specifically, `p1` should not yet introduce:

- `Vec<ResponseItem>` in the new crate contract
- artifact inventory payloads or artifact refs
- runtime-derived enrichments such as subagent snapshots

Those belong to the phases where the crate actually starts owning artifact
extraction, shaping, and codecs.

### 3. Pure route-matrix evaluator

Add a pure function in the new crate that maps:

- `MaintenanceAction`
- `MaintenanceTiming`
- `PolicyEngine`

to an intermediate route decision, then applies the governance overlay from:

- `GovernancePathVariant`

to produce a final `MaintenancePolicyPlan`.

This function should encode the **target** matrix from
[compaction-policy-route-matrix.md](compaction-policy-route-matrix.md), not the
current runtime deltas from `p0`.

In `p1`, this evaluator is a pure contract surface. It does not inspect
history, artifact payloads, or runtime session state.

### 4. Unsupported-route behavior

The new crate should fail closed at the planning level for unsupported routes.

At minimum:

- `Refresh + IntraTurn + any engine`
- `Prune + IntraTurn + any engine`

should return a typed planning error such as:

```rust
pub enum MaintenancePolicyError {
    UnsupportedRoute {
        action: MaintenanceAction,
        timing: MaintenanceTiming,
        engine: PolicyEngine,
    },
}
```

Use:

- `Result<MaintenancePolicyPlan, MaintenancePolicyError>`

Do not model an unsupported route as a valid plan with a boolean flag.

### 5. Crate-local route tests

Add tests in the new crate that assert the target matrix directly.

At minimum:

- `Compact + IntraTurn + LocalPure`
  - bridge requested
  - `thread_memory` not requested
  - `BeforeLastRealUserOrSummary`
- `Compact + TurnBoundary + LocalPure`
  - `thread_memory` requested
  - bridge not requested as durable carry-forward
  - `None`
- `Compact + TurnBoundary + LocalPure + GovernancePathVariant::Off`
  - route remains supported
  - governance overlay suppresses `thread_memory`
- `Compact + TurnBoundary + RemoteVanilla`
  - no fork artifacts requested
  - legacy marker preserved
- `Refresh + TurnBoundary + LocalPure`
  - `thread_memory` requested
  - bridge not requested
- `Refresh + TurnBoundary + RemoteVanilla`
  - unsupported
- `Refresh + IntraTurn + any`
  - unsupported
- `Prune + TurnBoundary + any`
  - no semantic artifact generation
  - prune-manifest-style marker path only

These tests are the target-law companion to the `p0` current-state tests.

## Proposed File Changes

### Docs

Add:

- `docs/compaction-policy-p1-scaffold-implementation-spec.md`

Update links in:

- [compaction-policy-crate-extraction-plan.md](/home/rose/work/codex/fork/docs/compaction-policy-crate-extraction-plan.md)

### Rust

Add:

- `codex-rs/context-maintenance-policy/Cargo.toml`
- `codex-rs/context-maintenance-policy/src/lib.rs`
- `codex-rs/context-maintenance-policy/src/contracts.rs`
- `codex-rs/context-maintenance-policy/src/route_matrix.rs`
- `codex-rs/context-maintenance-policy/src/tests.rs`

Touch:

- `codex-rs/Cargo.toml`

Avoid touching `codex-core` in `p1` unless one of these becomes necessary:

- adding the workspace dependency entry only

Default expectation: `p1` should not require any `codex-core` source change.

## API Shape Guidance

### Keep naming self-describing

Avoid ambiguous bools or positional flags in the new crate API.

Good:

- `MaintenanceTiming::IntraTurn`
- `ContextInjectionPolicy::None`

Bad:

- `is_mid_turn: bool`
- `inject_context: bool`

### Keep artifact kinds narrow

The first version should only model the artifact families already named by the
route matrix:

- `ThreadMemory`
- `ContinuationBridge`
- `PruneManifest`
- `CompactionMarker`

Do not introduce a generic "artifact bag" abstraction yet.

### Defer rich request surfaces

`p1` should not lock in:

- raw `ResponseItem`-carrying request fields
- artifact inventory references
- runtime-derived enrichments

Those should be introduced only when the new crate starts using them in `p2`
and `p3`.

## Validation

For `p1`, run:

```bash
cd /home/rose/work/codex/fork/codex-rs
cargo test -p codex-context-maintenance-policy
just fmt
just fix -p codex-context-maintenance-policy
```

And keep the existing `p0` guardrails green:

```bash
cargo test -p codex-core compaction_policy_matrix_tests --lib
```

Do not run a full workspace test pass solely for `p1`.

## Exit Criteria

`p1` is complete when:

1. the new crate exists in the workspace
2. the typed contract compiles and is documented by code structure
3. the target route matrix is encoded in a pure evaluator
4. the evaluator has direct tests for supported and unsupported routes
5. no runtime behavior has changed yet

At that point the branch is ready for:

- `p2` pure history-shaping extraction
- and, if desired, a draft PR to start review on the scaffold

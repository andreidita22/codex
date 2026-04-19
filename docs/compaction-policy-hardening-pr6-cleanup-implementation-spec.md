# Compaction Policy Hardening PR 6 Implementation Spec

## Purpose

This document defines the final optional cleanup slice for the
context-maintenance hardening family.

PR 1 through PR 5 already sealed the mandatory law surfaces:

- policy-owned governance axis
- executable artifact requests
- unified disposition
- timing/injection decoupling
- policy-owned retention directives

PR 6 is intentionally narrower. It is for boundary polish and ownership
cleanup, not new route semantics.

This spec covers the three remaining reasonable cleanup items as one small PR:

1. move continuation-bridge supplemental subagent context ownership into the
   policy crate
2. narrow the public exports of `codex-context-maintenance-policy`
3. remove the duplicate `content_items_to_text` helper surface

## Why This PR Exists

The current merged state is functionally sound, but a few low-risk seams remain:

- continuation-bridge prompt assembly is policy-owned, while the
  `continuation_bridge_subagents` supplemental developer block is still built in
  `codex-core`
- the policy crate exports a much broader surface than the core adapter
  actually needs
- `content_items_to_text` still exists in both `codex-core` and the policy
  crate

These are not correctness blockers. They are cleanup-grade ownership issues
that are worth fixing while the extraction shape is still fresh.

## Non-Goals

PR 6 must not:

- change the route matrix
- change artifact lifetime or requiredness semantics
- change timing classification or injection policy
- redesign continuation-bridge payload shape
- widen the public API of the policy crate
- absorb unrelated observability or sub-agent E-witness work

If a change starts affecting runtime law rather than ownership/polish, it does
not belong in PR 6.

## Cleanup Item 1: Bridge Supplemental Ownership

### Goal

Move ownership of continuation-bridge supplemental subagent context into the
policy crate so the bridge family is fully owned there.

### Current Problem

Today:

- [codex-rs/core/src/continuation_bridge.rs](/home/rose/work/codex/fork/codex-rs/core/src/continuation_bridge.rs)
  owns runtime execution and model streaming, which is correct
- but it also still builds `supplemental_items` locally
- [codex-rs/core/src/continuation_bridge/subagent_context.rs](/home/rose/work/codex/fork/codex-rs/core/src/continuation_bridge/subagent_context.rs)
  still owns the tagged `continuation_bridge_subagents` payload format

That leaves one bridge-family developer artifact shape outside the policy crate.

### Required Outcome

The policy crate should own:

- the `continuation_bridge_subagents` tag constant
- the canonical JSON field schema for the supplemental subagent payload
- the tagged supplemental block format for subagent context
- the helper that turns a typed subagent snapshot list into a supplemental
  `ResponseItem`
- the bridge prompt-input assembly contract including supplemental items

`codex-core` should still own:

- gathering live subagent state from the session/runtime
- translating runtime subagent state into a minimal typed snapshot input
- model execution and streaming

It must not require the policy crate to depend on core runtime types.

The target shape is:

- core collects typed subagent snapshots
- core passes them into a policy-owned supplemental-item builder
- policy returns the bridge-family `ResponseItem` for prompt assembly

### Recommended API Direction

Introduce a policy-owned typed input such as:

```rust
pub struct ContinuationBridgeSubagentSnapshot {
    pub agent_id: String,
    pub thread_id: String,
    pub nickname: String,
    pub role: String,
    pub status: ContinuationBridgeSubagentStatus,
}

pub enum ContinuationBridgeSubagentStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

pub fn continuation_bridge_subagent_context_item(
    snapshots: Vec<ContinuationBridgeSubagentSnapshot>,
) -> CodexResult<Option<ResponseItem>>;
```

The exact names can vary, but the rule is:

- typed snapshot input comes from core
- tagged payload shape is owned by policy
- status should be policy-owned typed data, not a free `String`

### Expected Touched Surface

- [codex-rs/context-maintenance-policy/src/continuation_bridge/mod.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/continuation_bridge/mod.rs)
- [codex-rs/context-maintenance-policy/src/lib.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/lib.rs)
- [codex-rs/core/src/continuation_bridge.rs](/home/rose/work/codex/fork/codex-rs/core/src/continuation_bridge.rs)
- [codex-rs/core/src/continuation_bridge/subagent_context.rs](/home/rose/work/codex/fork/codex-rs/core/src/continuation_bridge/subagent_context.rs)

Expected fate of
[codex-rs/core/src/continuation_bridge/subagent_context.rs](/home/rose/work/codex/fork/codex-rs/core/src/continuation_bridge/subagent_context.rs):

- either delete it entirely, or
- reduce it to a thin runtime-only snapshot collector/translator

It should not continue to own the tagged payload format after PR 6.

### Validation

- policy crate tests for the new supplemental-item builder
- exact-output golden test for the supplemental developer block tag and JSON
  field shape
- targeted `codex-core` continuation-bridge tests
- `cargo test -p codex-context-maintenance-policy`
- `cargo test -p codex-core continuation_bridge --lib`

### Behavioral Delta

No intended visible behavior change.

This is an ownership move only.

## Cleanup Item 2: Narrow Policy-Crate Public Exports

### Goal

Reduce the public surface of `codex-context-maintenance-policy` to the actual
core-facing seam.

### Current Problem

[codex-rs/context-maintenance-policy/src/lib.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/lib.rs)
currently re-exports many internal helpers, intermediate types, and constants
that are not part of the intended long-term adapter seam.

Examples include:

- internal result wrapper types
- artifact payload enums used only inside one family
- raw prompt/schema constants
- low-level helper functions that are only used inside tests or same-crate
  modules

This makes the crate look more stable and more generic than it really is.

### Required Outcome

Keep public only what is needed by `codex-core` and by legitimate crate-level
tests.

Prefer:

- private modules
- item-level `pub` only for true external seam functions/types
- `pub(crate)` inside the policy crate for internal-family wiring

It is acceptable to keep some items public if making them private would force
awkward module plumbing, but the default direction should be to narrow the API.

Use an allowlist mindset:

- planning contract types/functions
- artifact-family seam functions actually consumed by core
- deterministic plan/history helpers actually consumed by core

Everything else should need a concrete cross-crate justification to remain
public.

### Specific Candidates To Review

Examples that are good candidates for narrowing if no cross-crate caller needs
them:

- `ContinuationBridgePayload`
- `HistoryDispositionResult`
- `RetentionApplicationResult`
- `THREAD_MEMORY_PROMPT`
- `THREAD_MEMORY_SCHEMA`
- low-level insertion helpers if only core wrapper functions call them

Avoid keeping raw prompt/schema constants public unless core actually imports
them directly.

This list is illustrative, not exhaustive. Use actual call-site grep before
changing visibility.

### Expected Touched Surface

- [codex-rs/context-maintenance-policy/src/lib.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/lib.rs)
- any policy-crate modules whose item visibility needs to change
- any `codex-core` imports that should switch to the narrower seam

### Validation

- `cargo test -p codex-context-maintenance-policy`
- targeted `codex-core` tests for affected import paths

### Behavioral Delta

No intended runtime behavior change.

This is API-surface cleanup only.

## Cleanup Item 3: Remove Duplicate `content_items_to_text`

### Goal

Pick one canonical owner for `content_items_to_text` and remove the duplicate
surface.

### Current Problem

The helper currently exists in both:

- [codex-rs/context-maintenance-policy/src/artifact_codecs.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/artifact_codecs.rs)
- [codex-rs/core/src/compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)

The implementations are currently equivalent, but duplicate utility helpers
drift over time.

### Required Outcome

There should be one canonical implementation.

Recommended direction:

- keep the policy-crate implementation
- switch remaining core callers to import it from the policy crate
- remove the duplicate helper from `codex-core`
- remove any now-dead re-exports from `codex-core` if they are only there for
  this helper

The helper should be single-sourced, but not more public than necessary. If the
remaining cross-crate caller set is narrow, expose it through the smallest
reasonable seam rather than leaving it as a broad utility export.

If a repo-wide public re-export from `codex-core` is still intentionally
needed, document the reason and keep the implementation single-sourced behind
that re-export.

### Expected Touched Surface

- [codex-rs/context-maintenance-policy/src/artifact_codecs.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/artifact_codecs.rs)
- [codex-rs/context-maintenance-policy/src/lib.rs](/home/rose/work/codex/fork/codex-rs/context-maintenance-policy/src/lib.rs)
- [codex-rs/core/src/compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)
- any remaining `codex-core` users of `crate::compact::content_items_to_text`
- possibly [codex-rs/core/src/lib.rs](/home/rose/work/codex/fork/codex-rs/core/src/lib.rs)

### Validation

- direct compatibility test proving the retained implementation matches current
  `content_items_to_text` behavior
- targeted tests that already cover `content_items_to_text` behavior
- `cargo test -p codex-context-maintenance-policy`
- `cargo test -p codex-core compact::tests --lib`

### Behavioral Delta

No intended behavior change.

This is deduplication only.

## Recommended Implementation Order

Implement PR 6 in this order:

1. move bridge supplemental artifact shape ownership into policy
2. switch core to consume the new policy helper
3. delete the duplicate `content_items_to_text` surface
4. narrow policy-crate exports based on the final remaining cross-crate callers

This order keeps the export narrowing grounded in actual post-cleanup call
sites instead of guessing up front.

## Testing Layers

This PR should keep the test layers clear:

- policy crate tests:
  - new bridge supplemental-item builder coverage
  - any visibility-safe refactor coverage
- core targeted tests:
  - continuation-bridge tests
  - compact/helper tests affected by the deduplicated text helper
- no broad route-law or live-behavior snapshot changes are expected

## Success Criteria

PR 6 is complete when:

- continuation-bridge supplemental tagged payload shape is owned by the policy
  crate, not by core
- the policy crate public surface is measurably narrower than today
- `content_items_to_text` has one canonical implementation
- no route semantics changed
- targeted tests remain green

## Explicit Scope Guard

If implementation pressure reveals semantic drift, stop and split the work.

PR 6 should remain:

- ownership cleanup
- API narrowing
- helper deduplication

It must not become a stealth PR that changes the context-maintenance law.

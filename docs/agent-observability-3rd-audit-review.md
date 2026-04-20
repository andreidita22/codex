# Agent Observability 3rd Audit Review

This note reviews [gptpro 3rd audit.md](/home/rose/work/codex/fork/docs/gptpro%203rd%20audit.md) after narrowing scope to the E-witness / sub-agent observability feature family only.

The cloud-task portion of that audit is intentionally not carried forward here. For the current fork state, it is not the right next modularization target.

## Judgment

The audit is useful and mostly right once scoped down to E-witness.

The main conclusion I accept is:

- the next worthwhile extraction target is the progress/reducer law in [progress.rs](/home/rose/work/codex/fork/codex-rs/core/src/agent/progress.rs), not a broad "multi-agent" or "collaboration" crate

The strongest remaining issues are real:

1. reducer law still lives in `codex-core`
2. progress tool names and phase/schema strings are still duplicated across `core` and `tools`
3. thread-spawn tool-surface containment is real policy, but it is still encoded as booleans rather than a named planning object

Those three points are enough to justify a focused follow-up family.

## Accepted Findings

### 1. The real seam is observability law, not all sub-agent control

This is the most important framing decision in the audit.

The feature boundary worth extracting is:

- normalized sub-agent progress observation
- wait semantics over progress state
- model-visible progress tool contract

The boundary is not:

- all of `AgentControl`
- all multi-agent collaboration
- thread lifecycle orchestration
- generic tool registration

That is the right cut.

### 2. Reducer law should move out of `codex-core`

[progress.rs](/home/rose/work/codex/fork/codex-rs/core/src/agent/progress.rs) is fork-owned semantic logic living in a hot upstream crate.

The file currently owns:

- progress phases
- blocker classification
- active-work state
- progress snapshots
- registry behavior
- event-to-progress reduction
- sequence advancement rules
- stall calculation

That is exactly the kind of durable law surface that benefited from extraction in context-maintenance.

### 3. Tool contract drift risk is real

[agent_progress_tool.rs](/home/rose/work/codex/fork/codex-rs/tools/src/agent_progress_tool.rs) still carries manually repeated phase/schema information that should come from the semantic owner.

[router.rs](/home/rose/work/codex/fork/codex-rs/core/src/tools/router.rs) still hardcodes progress-tool names as string literals.

Those are concrete ownership leaks, not abstract architecture concerns.

### 4. Thread-spawn containment should stay in `codex-tools`, but be made explicit

The audit is right that thread-spawn tool-surface law is real. It is also right that it does not belong in the future observability crate.

That policy should stay with tool planning in `codex-tools`, but it should be named and testable instead of being left as paired booleans in config/planning code.

### 5. Lifecycle status is protocol-owned, not core-owned

One important refinement to the audit framing:

- lifecycle status itself is not a `codex-core` concept
- the relevant type comes from `codex-protocol`

So the future observability crate can legitimately consume protocol-owned `AgentStatus` without creating a core dependency.

What must remain adapter-side is:

- retrieving lifecycle status from live thread/runtime state

What should move into the semantic owner is:

- deciding whether a lifecycle status is terminal for progress purposes
- applying lifecycle status to snapshot/phase semantics

## Adjustments To The Audit

### 1. Do not adopt the proposed public API too literally

The audit sketches a reasonable crate shape, but its example API should not be copied mechanically if it drags core-owned lifecycle concepts into the new crate.

In particular:

- the new crate should not depend on `codex-core`
- runtime retrieval of lifecycle status should remain adapter-side
- runtime waiting behavior should remain handler-side

The crate should own semantic classification and progress state, not session/runtime orchestration.

It may still own semantic interpretation of protocol-owned lifecycle status.

### 2. Keep cloud out of this family

The audit's cloud section is fine as a scoping correction, but it is not actionable modularization work for this next family.

For current planning purposes:

- the E-witness line is the next real target
- cloud should be treated as upstream-owned unless future fork deltas appear again

### 3. Test cleanup is lower value than the semantic move

The audit is right that core tests still duplicate doctrine. That matters, but it should not lead the work.

The right order is:

1. move semantic ownership
2. single-source tool contract constants
3. name tool-surface policy
4. then reduce duplicate doctrine in core tests

## Decisions Carried Forward

These decisions should guide the next extraction:

- create a dedicated crate for observability law, not a generic fork hub
- keep tool registry planning in `codex-tools`
- keep handler/runtime execution in `codex-core`
- treat thread-spawn containment as `codex-tools` policy, not observability-crate policy
- single-source all observability-owned model-visible enum/string contracts from the semantic owner
- keep protocol-owned lifecycle status available to the new crate without introducing a core dependency

Preferred crate name:

- `codex-agent-observability`

That is better than a narrower `subagent` name because the reducer semantics are about agent-thread progress generally, even if the current feature is sub-agent focused.

## Recommended Next Family

The next family should be:

1. behavior locks around progress law and containment
2. create `codex-agent-observability` and move/adopt reducer/types/registry in one copy-free step
3. harden wait semantics and snapshot accessors around the new owner
4. single-source progress tool contract
5. make thread-spawn tool-surface policy explicit in `codex-tools`
6. clean up duplicated doctrine and narrow public API only after the ownership move

That is the right follow-up to the context-maintenance pattern and the right use of the third audit.

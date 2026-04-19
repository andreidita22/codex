# Compaction Policy Second Audit Review

## Purpose

This document records the review of:

- [docs/gptpro 2nd audit.md](gptpro%202nd%20audit.md)

It exists so future work does not need to reconstruct which findings were
accepted, which were deferred, and why.

## Executive Read

The second audit is useful and mostly accurate.

Its strongest findings are real remaining boundary leaks:

1. artifact source selection is still incomplete
2. history disposition is still partially caller-selected in core
3. generic protocol utilities still live in the policy crate

Those three are worth another hardening round.

Several other findings are true observations but lower-value:

- governance-off semantics are under-specified
- artifact lifetime is stronger as a type than as an enforced invariant
- policy public API is still somewhat broad
- tests still duplicate some doctrine

Those should not drive the next round unless they fall naturally out of the
higher-value work.

## Accepted Findings

### 1. Artifact Source Selection Is Still Incomplete

Status:

- accepted
- high priority

Why it is real:

- `ThreadMemory` and `ContinuationBridge` lifetime/carry rules are fully
  enforced only in final replacement history, not in model input.
- `context-maintenance-policy/src/thread_memory.rs` still falls back to
  `input.to_vec()` when no previous thread memory exists.
- `core/src/continuation_bridge.rs` still feeds raw prompt history into bridge
  generation with no policy-owned source filter.

Why it matters:

- a prior turn-scoped artifact can still influence the next artifact before it
  is dropped from final history
- this weakens the intended meaning of `ArtifactLifetime` and
  `drop_prior_artifact_kinds`

Decision:

- do a dedicated source-selection hardening pass
- treat this as the highest-value remaining semantic improvement

### 2. History Disposition Is Still Partially Caller-Selected In Core

Status:

- accepted
- high priority

Why it is real:

- `context-maintenance-policy` owns artifact drops, marker policy, and
  retention, but core still chooses some disposition behavior:
  - `prune_superseded_artifacts`
  - prune summary retention
  - remote compact output keep/drop predicate

Why it matters:

- this leaves a second history-law surface in `codex-core`
- prune-manifest counts and summary-retention outcomes can drift if their
  accounting does not follow the same disposition result
- future artifact families or summary forms would still reopen core hot files

Decision:

- continue hardening the disposition boundary
- move more of this law behind policy-owned plan data plus narrow classifier
  callbacks from core
- keep prune-manifest accounting aligned with the same plan-driven disposition
  result

### 3. `content_items_to_text` Is Single-Sourced But Owned By The Wrong Crate

Status:

- accepted
- medium priority

Why it is real:

- the helper now lives in `context-maintenance-policy`, but unrelated core
  modules use it via `codex-core`
- that makes the policy crate a generic utility provider

Why it matters:

- it broadens policy ownership beyond context-maintenance semantics
- it creates an avoidable dependency direction from generic core code into the
  policy crate

Decision:

- move the helper into `codex-protocol::models`
- make both policy and core import it from protocol

### 4. Thread-Memory Trimming Still Owns Protocol Pairing Law

Status:

- accepted
- medium priority

Why it is real:

- `context-maintenance-policy/src/thread_memory.rs` still knows which
  `ResponseItem` variants pair with which outputs
- that is protocol semantics, not context-maintenance semantics

Why it matters:

- upstream protocol changes would still require policy-crate updates
- this is exactly the kind of upstream-ingest seam the extraction was meant to
  reduce

Decision:

- move pairing helpers into `codex-protocol`
- keep trimming policy-owned but delegate pair identity to protocol

### 5. Summary-Boundary Detection Is Still Stringly

Status:

- accepted
- fold into source-selection work

Why it is real:

- `split_previous_memory_and_source_items(...)` still takes a raw
  `summary_prefix: &str` from core

Why it matters:

- compact summary filtering in policy still depends on a core template string
- this is a small but real adapter leak

Decision:

- do not treat this as a standalone PR
- fold it into the source-selection hardening work as a classifier callback or
  equivalent adapter shape

## Deferred Or Conditional Findings

### 6. Governance-Off Behavior Is Under-Specified

Status:

- real observation
- deferred until product semantics are clarified

Why it is not immediate:

- current behavior is coherent if `Disabled` means
  “do not generate new thread memory”
- the audit’s alternative meaning
  (“drop and ignore all thread memory”) is plausible, but not obviously more
  correct

Decision:

- do not change behavior just because the audit noticed the ambiguity
- if we touch this later, do it as an explicit semantics decision

### 7. Artifact Lifetime Is Declared More Strongly Than It Is Enforced

Status:

- true
- mostly derivative of source-selection incompleteness

Why it is not separate:

- if source selection is hardened and route invariants are improved, this issue
  becomes much smaller

Decision:

- do not start a standalone lifetime framework
- treat this as a follow-on check within source-selection hardening

## Lower-Value Cleanup Findings

### 8. Policy Public API Is Still Wider Than Needed

Status:

- true
- lower priority than the accepted semantic leaks

Decision:

- worth cleanup later
- not a reason to delay the more important remaining hardening

### 9. Unsupported-Route Error Wording Is Duplicated

Status:

- true
- very low priority

Decision:

- trivial cleanup only
- do not give it its own workstream

### 10. Tests Still Duplicate Some Doctrine

Status:

- true
- cleanup-grade

Decision:

- reduce duplication when it naturally falls out of later refactors
- do not start with test churn before the remaining law surfaces are fixed

## Recommended Follow-Up Order

The best next sequence is:

1. source-selection hardening
2. disposition-boundary hardening
3. protocol utility relocation
4. semantics clarification and cleanup

More specifically:

1. add policy-owned source selectors for `ThreadMemory` and
   `ContinuationBridge`
2. move more history-disposition choices into policy-owned plan data, including
   prune-manifest accounting alignment
3. move `content_items_to_text` and response-item pair helpers into
   `codex-protocol`
4. only then revisit:
   - governance-off semantics
   - API narrowing
   - test de-duplication
   - minor error-message cleanup

## Bottom Line

The second audit should influence the next plan.

But it should do so selectively:

- trust the top three structural findings
- make turn-scoped artifacts excluded from generation and carry-forward unless
  a route explicitly opts in
- treat governance/lifetime wording as second-order effects
- keep minor API/test/error cleanup in the background unless it naturally fits
  the larger follow-up work

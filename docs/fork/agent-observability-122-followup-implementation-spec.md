# Agent Observability 0.122 Follow-Up Implementation Spec

This spec captures the narrow post-ingest follow-up implied by
[gptpro-audit-05-upstream-0.122-ingest.md](gptpro-audit-05-upstream-0.122-ingest.md).

Spec-tightening feedback for this document is recorded in
[gptpro-audit-06-observability-122-followup-spec-review.md](/home/rose/work/codex/fork/docs/fork/gptpro-audit-06-observability-122-followup-spec-review.md).

It is intentionally separate from the original extraction family in
[agent-observability-implementation-spec.md](/home/rose/work/codex/fork/docs/fork/agent-observability-implementation-spec.md).

That family already landed:

- reducer and registry ownership moved to `codex-agent-observability`
- wait semantics moved behind the observability seam
- tool contract strings became single-sourced
- `AgentToolSurfacePolicy` became explicit in `codex-tools`

This follow-up is not another extraction family. It is a 0.122-specific
adapter hardening pass caused by upstream event-shape and runtime-delivery
changes.

## Problem Statement

Upstream `0.122` changed the event surface in two important ways:

1. new primary structured progress events became first-class:
   - `ReasoningContentDelta`
   - `ReasoningRawContentDelta`
   - `ItemStarted`
   - `ItemCompleted`
2. the runtime split moved the progress-recording hook to `Session::send_event`
   instead of the old raw-delivery boundary

That means the current fork has a likely semantic gap:

- the reducer still mostly understands legacy-style progress events
- generated legacy events are emitted for compatibility but are no longer
  observed by the reducer
- direct `send_event_raw(...)` call sites can now bypass progress observation
  entirely

The result is not a crate-boundary failure. The observability crate still owns
the right law. The issue is that the session adapter and reducer are no longer
aligned to the `0.122` event model.

## Follow-Up Invariant

Observability state must be updated exactly once for every progress-relevant
runtime event.

That means:

- primary structured events should be observed directly
- generated legacy compatibility events should not become the semantic source of
  truth
- direct raw event sends must not silently bypass progress observation when the
  event is lifecycle-relevant or progress-relevant

The fix must not move semantic law back into `codex-core`.

That means:

- `codex-agent-observability` should learn the `0.122` primary events
- `codex-core` should only decide whether an event is observed once or emitted
  as compatibility-only output
- `codex-tools` should remain unchanged unless a wait/progress behavior test
  proves otherwise

## Scope

In scope:

- primary structured event support in
  `codex-rs/agent-observability/src/progress.rs`
- explicit progress-observation semantics at the session event boundary
- regression tests around wait/progress wakeups and direct raw event paths

Out of scope:

- new extraction work
- context-maintenance changes
- `ThreadMemoryGovernance::Disabled`
- large `session/*` refactors
- tool-planning redesign
- TUI slash-command cleanup unrelated to observability

## PR 1 — Structured Primary Event Support

### Purpose

Teach `codex-agent-observability` the `0.122` primary event forms so progress
semantics no longer depend on generated legacy compatibility events.

### Required reducer coverage

Add explicit handling for at least:

- `EventMsg::ReasoningContentDelta`
- `EventMsg::ReasoningRawContentDelta`
- `EventMsg::ItemStarted`
- `EventMsg::ItemCompleted`

### Required semantic mapping

Minimum mapping rules:

- `ReasoningContentDelta`
  - same progress meaning as `AgentReasoningDelta`
- `ReasoningRawContentDelta`
  - must be explicitly classified as intentionally ignored for material
    progress
  - it must not remain under wildcard handling
  - do not promote raw reasoning to material progress in this follow-up unless
    that behavior change is deliberately approved later
- `ItemStarted(TurnItem::WebSearch)`
  - same meaning as `WebSearchBegin`
- `ItemCompleted(TurnItem::WebSearch)`
  - same meaning as `WebSearchEnd`
- `ItemStarted(TurnItem::ImageGeneration)`
  - same meaning as `ImageGenerationBegin`
- `ItemCompleted(TurnItem::ImageGeneration)`
  - same meaning as `ImageGenerationEnd`
- `ItemCompleted(TurnItem::AgentMessage)`
  - same visible-message semantics as `AgentMessage`
  - at most one material milestone for the completed item
  - if content has multiple text chunks, normalize them into one visible-message
    update
  - do not emit one milestone per content chunk
  - do not treat this event as the owner of `final_message`; final completion
    still belongs to terminal turn completion
- `ItemCompleted(TurnItem::Reasoning)`
  - same reasoning-summary semantics as `AgentReasoning`
  - use `summary_text` only
  - if `summary_text` is empty, this is an explicit no-op for material progress
  - do not use `raw_content` as a material progress signal
- `PlanDelta`
  - explicitly ignored for progress in this follow-up unless a separate
    behavior-change decision promotes plan updates into progress state

Implementation should use explicit helpers such as:

- `record_item_started(...)`
- `record_item_completed(...)`

Those helpers should match deliberately on every current `TurnItem` variant.

Most variants may remain explicit no-ops, but this PR should avoid a catch-all
that hides future upstream additions.

All other `ItemStarted` / `ItemCompleted` variants touched by this PR should be
classified deliberately:

- either ignored explicitly
- or mapped explicitly

Do not leave them under a catch-all wildcard if they are relevant to progress.

### Testing

Add reducer-level tests in `codex-agent-observability` that exercise the
primary structured events directly.

Do not rely on `as_legacy_events(...)` in these tests.

Required reducer coverage includes:

- `ReasoningContentDelta` promotes initial reasoning progress and later deltas
  stay heartbeat-only
- `ReasoningRawContentDelta` is explicitly ignored for material progress
- `ItemStarted(WebSearch)` sets tool-call progress and `web_search` active work
- `ItemCompleted(WebSearch)` clears tool work and records completion
- `ItemStarted(ImageGeneration)` sets tool-call progress and
  `image_generation` active work
- `ItemCompleted(ImageGeneration)` clears tool work and records completion
- `ItemCompleted(AgentMessage)` produces at most one milestone
- `ItemCompleted(Reasoning)` uses summary text only
- non-progress `TurnItem` variants are explicit no-ops

### Scope guard

Do not change the session observation hook in this PR.

This PR should make the reducer `0.122`-aware first.

## PR 2 — Explicit Session Observation Boundary

### Purpose

Make progress observation explicit at the session event boundary so direct raw
event sends cannot silently bypass the observability registry.

### Problem to fix

Today:

- `Session::send_event(...)` records progress on the primary event
- generated legacy events are emitted via `send_event_raw(...)`
- `send_event_raw(...)` and `deliver_event_raw(...)` do not record progress

That is too implicit for the `0.122` runtime shape.

### Required end state

The adapter must support two distinct concepts:

- observed runtime event
- compatibility-only emitted event

The API should be explicit, with a shape equivalent to:

```rust
enum ProgressObservation {
    Observe,
    CompatibilityOnly,
}
```

and raw delivery must require that choice explicitly.

Do not keep an unqualified production `send_event_raw(event)` boundary after
this PR lands.

### Required behavior

- primary events emitted through `send_event(...)` are observed exactly once
- generated legacy compatibility events are emitted with
  `ProgressObservation::CompatibilityOnly`
- normal direct raw runtime events should be observed by default
- compatibility-generated legacy echoes should be the only normal unobserved
  raw path
- the existing pre-raw `record_progress_event(...)` call in `Session::send_event`
  must move into the observed raw boundary or be removed; do not allow a final
  shape where primary events are observed twice

### Minimum direct raw paths to review

Review `send_event_raw(...)` and `deliver_event_raw(...)` callers across:

- `core/src/session/handlers.rs`
- `core/src/session/mod.rs`
- `core/src/session/session.rs`
- `core/src/session/turn_context.rs`

At minimum, ensure explicit handling for raw paths that emit:

- `Error`
- `ShutdownComplete`
- any other lifecycle/progress-visible event that should wake progress waiters

`Warning` paths should still be reviewed, but do not assert that warnings wake
waiters unless reducer semantics are deliberately changed to bump sequence for
warnings.

### Boundary rule

Do not reintroduce semantic event classification into `codex-core`.

`codex-core` should only choose:

- observe once
- emit without observation

The reducer should remain the semantic owner of what the event means.

`ProgressObservation` must not silently change rollout persistence behavior.

`CompatibilityOnly` means:

- do not update `ProgressRegistry`

It does not mean:

- do not persist
- do not deliver

For observed raw delivery, update lifecycle status from the event before
notifying `ProgressRegistry`, so waiters wake into a consistent
status-plus-snapshot view.

### Testing

Add focused adapter tests in `codex-core` that prove:

- primary `send_event(...)` still advances progress once
- generated legacy events do not double-count sequence
- relevant raw lifecycle/error paths do not bypass progress observation
- direct raw `Error` updates terminal progress state
- direct raw `ShutdownComplete` updates terminal progress state
- non-progress direct raw events can still be observed without changing
  sequence

Also treat this as a compile-time success condition:

- no production call site can use raw event delivery without specifying
  observation mode

### Scope guard

Do not broaden this into a general event-delivery refactor.

This PR is only about making the observation boundary explicit and safe.

## PR 3 — Wait / Progress Regression Tests

### Purpose

Lock the post-`0.122` observability behavior end to end after reducer support
and session-boundary hardening land.

### Required coverage

Add regression tests that prove:

- waiting on reasoning progress wakes on `ReasoningContentDelta`
- waiting on tool progress wakes on:
  - `ItemStarted(WebSearch)`
  - `ItemStarted(ImageGeneration)`
- waiting on terminal/lifecycle paths does not stall until timeout just because
  the event traveled through a raw-send path
- generated legacy compatibility events do not cause a second wake or sequence
  increment

Prefer exact count / exact sequence assertions over tests that merely prove a
wait eventually woke.

### Preferred test layers

Reducer-law tests:

- `codex-agent-observability`

Wait-handler/runtime tests:

- `codex-core/src/tools/handlers/agent_progress.rs`
- targeted `codex-core` integration tests where needed

### Explicit non-goals

Do not use this PR to rename wait result types or redesign the tool schema.

This PR is for regression protection only.

## Validation

Expected minimum validation during the family:

```text
cargo test -p codex-agent-observability
cargo test -p codex-core tools::handlers::agent_progress::tests --lib
cargo check -p codex-core
```

Expected targeted additions as the PRs land:

```text
cargo test -p codex-core inspect_agent_progress_reports_reasoning_phase_for_live_subagent --lib
cargo test -p codex-core wait_for_agent_progress_returns_on_material_progress --lib
```

If changes touch `codex-core`, ask before running the complete workspace suite
or full `cargo test`, per repo policy.

## Recommendation

This follow-up should proceed only after the `0.122` ingest baseline itself is
stable enough to review cleanly.

It is a real fork-relevant follow-up because it affects:

- the semantics of the extracted observability module
- the safety of the new `session/*` adapter boundary
- the reliability of `inspect_agent_progress` /
  `wait_for_agent_progress`

But it should stay narrow.

The right result is:

- observability law remains in `codex-agent-observability`
- session delivery becomes explicit about observation
- no semantic ownership drifts back into `codex-core`

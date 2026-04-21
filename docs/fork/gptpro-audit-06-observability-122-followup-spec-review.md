## Verdict

The spec is the right follow-up slice. It correctly treats the 0.122 issue as **adapter/reducer alignment**, not a failed extraction or reason to move observability law back into `codex-core`. The core invariant is also right: primary structured events should become the semantic source of truth; generated legacy compatibility events should not drive reducer state; direct raw sends must not silently bypass progress observation. 

It should proceed, but it needs several tightening changes before implementation. The current spec leaves too much latitude around raw-event observation shape, raw reasoning semantics, direct warning/error behavior, and exact counting guarantees.

## What the spec gets right

The scope is clean. It does not reopen the original extraction, tool planning, context maintenance, TUI cleanup, or broad session refactors. That matches the accepted observability boundary: reducer, registry, wait semantics, and progress tool contract are observability-owned; session/runtime waiting and tool registration stay adapter-side.  

The PR ordering is mostly right:

1. make the reducer understand 0.122 primary structured events;
2. make session observation explicit;
3. add end-to-end wait/progress regression tests.

That is the right dependency order. Do not fix this by observing generated legacy events again.

The spec also correctly avoids semantic classification in `codex-core`: core should choose whether an event is observed or compatibility-only; the observability crate decides what that event means. 

## Required amendments

### 1. Make `ReasoningRawContentDelta` explicitly ignored unless you intend a behavior change

The spec currently says `ReasoningRawContentDelta` may either be treated like raw reasoning delta or intentionally ignored. That is too loose.

Current reducer behavior does not appear to treat raw reasoning content as material progress. Mapping raw reasoning deltas to `AgentReasoningDelta` would likely change behavior by waking waiters on raw chain-of-thought-ish content. That is not obviously desirable.

Recommended spec change:

```text
ReasoningRawContentDelta must be explicitly classified as intentionally ignored for material progress, unless a separate behavior-change decision says raw reasoning should wake progress waiters.
```

It can still be recorded as a heartbeat if there is a deliberate reason, but the spec should not leave the implementer to choose casually.

### 2. Specify exact `ItemCompleted(Reasoning)` semantics

“Same reasoning-summary/finalization semantics as `AgentReasoning`” is directionally right, but under-specified.

Current `TurnItem::Reasoning` can contain:

* `summary_text`
* `raw_content`

Recommended rule:

```text
ItemCompleted(TurnItem::Reasoning) should apply reasoning progress only from `summary_text`.
If `summary_text` is empty, it should be an explicit no-op for material progress.
Do not use `raw_content` for material progress unless raw reasoning is deliberately promoted.
```

This avoids accidentally making hidden/raw reasoning a progress signal.

### 3. Specify exact `ItemCompleted(AgentMessage)` counting

The spec says `ItemCompleted(TurnItem::AgentMessage)` should have the same final-message semantics as `AgentMessage`, but without duplicate milestone counting relative to message deltas. That needs a concrete rule.

Recommended rule:

```text
ItemCompleted(TurnItem::AgentMessage) should produce at most one material milestone.
If message content has multiple text chunks, concatenate or otherwise normalize them into one visible-message update.
Do not emit one progress milestone per content chunk.
```

Also do not set `final_message` from this event unless that is already what `AgentMessage` does. Final completion should still come from `TurnComplete`.

### 4. Require explicit `TurnItem` exhaustiveness inside the reducer helper

The spec says other `ItemStarted` / `ItemCompleted` variants “touched by this PR” should be classified. That leaves too much room for wildcard handling.

Use a helper shape that forces deliberate classification of every current `TurnItem` variant:

```rust
fn record_item_started(...)
fn record_item_completed(...)
```

and match explicitly on:

* `UserMessage`
* `HookPrompt`
* `AgentMessage`
* `Plan`
* `Reasoning`
* `WebSearch`
* `ImageGeneration`
* `ContextCompaction`

Most can be explicit no-ops. The point is to make upstream additions visible at compile/review time instead of hiding under `_ => {}`.

### 5. Mention `PlanDelta` explicitly

`PlanDelta` is another 0.122 structured event. The spec does not need to change plan behavior, but it should require an explicit decision:

```text
PlanDelta remains intentionally ignored by progress observation unless a separate decision promotes plan updates into progress state.
```

Leaving it under wildcard is the same class of future-ingest risk as the primary events called out in the spec.

### 6. PR2 needs a concrete API shape, not “equivalent to”

The current text says the exact API shape may vary. For this boundary, that is too loose. The implementation should make unobserved compatibility output impossible to confuse with observed runtime events.

Recommended shape:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProgressObservation {
    Observe,
    CompatibilityOnly,
}
```

Then require raw delivery functions to take it:

```rust
send_event_raw(event, ProgressObservation::Observe)
send_event_raw(event, ProgressObservation::CompatibilityOnly)

deliver_event_raw(event, ProgressObservation::Observe)
deliver_event_raw(event, ProgressObservation::CompatibilityOnly)
```

or use two clearly named wrappers:

```rust
send_observed_event_raw(...)
send_compatibility_event_raw(...)
deliver_observed_event_raw(...)
deliver_compatibility_event_raw(...)
```

Do not keep an unqualified `send_event_raw(event)` callable from arbitrary sites after PR2. That would preserve the bypass.

### 7. Observe direct raw sends by default; mark only compatibility legacy events as unobserved

The spec currently says direct raw sends must make an explicit observation choice and lists `Error`, `Warning`, `ShutdownComplete`, and other lifecycle/progress-visible events for review. That can accidentally force core into event semantic classification.

Cleaner rule:

```text
All normal direct raw runtime events should be observed.
Generated legacy compatibility events should be the only default unobserved path.
```

Then the reducer can ignore irrelevant events. Core does not need to know whether `ListSkillsResponse`, `ThreadNameUpdated`, or `Warning` is progress-material.

This keeps the semantic boundary clean:

* core chooses “runtime event” vs “compatibility echo”;
* observability chooses what progress means.

### 8. Remove the current pre-raw progress recording from `send_event`

PR2 should explicitly say that the existing `record_progress_event(...)` call at the top of `Session::send_event(...)` must move into the observed raw boundary or be otherwise removed.

Bad final shape:

```rust
send_event() records progress
send_event_raw(..., Observe) records progress again
```

Required final shape:

```text
primary send_event path observes exactly once through the observed raw boundary;
generated legacy events go through compatibility-only raw boundary;
direct raw runtime events go through observed raw boundary.
```

### 9. Preserve rollout persistence separately from observation

The spec talks about observation but not persistence. Current raw send persists rollout items before delivery. Generated legacy compatibility events may still need to be persisted exactly as before even if they are not observed.

Add a rule:

```text
ProgressObservation must not silently change rollout persistence behavior.
CompatibilityOnly means “do not update ProgressRegistry,” not “do not persist or deliver.”
```

If persistence behavior is intentionally changed, that should be a separate decision.

### 10. Update lifecycle status before waking progress waiters

This is subtle but important. If an observed terminal event updates `ProgressRegistry` before `agent_status`, a waiter can wake on the progress sequence and inspect a stale lifecycle status.

Recommended PR2 rule:

```text
For observed raw delivery, update `agent_status` from the event before notifying `ProgressRegistry`.
```

Then `wait_for_agent_progress` wakes into a consistent snapshot/status view.

### 11. Do not test `Warning` as a wake event unless reducer semantics change

The spec lists raw paths emitting `Warning` among minimum direct raw paths to review. That is fine. But current reducer-style behavior treats warnings as recent updates, not material progress. They do not necessarily bump sequence or wake waiters.

Recommended wording:

```text
Error and ShutdownComplete must wake waiters because they are terminal/material.
Warning should be observed or explicitly ignored according to current reducer semantics, but should not be asserted as a wait wakeup unless the reducer is deliberately changed to bump sequence for warnings.
```

### 12. PR3 should not be the first place reducer/adapter tests appear

PR3 is fine as an end-to-end regression layer, but PR1 and PR2 must carry their own tests.

Required:

* PR1 reducer tests prove primary structured events reduce correctly.
* PR2 adapter tests prove exact-once observation and no legacy double-count.
* PR3 adds user-visible wait/progress regression coverage.

Do not defer PR1/PR2 correctness into PR3.

## PR-specific review

### PR 1 — Structured primary event support

Keep this PR purely inside `codex-agent-observability`.

Required tests should include:

* `ReasoningContentDelta` promotes initial reasoning progress and later deltas are heartbeat-only.
* `ReasoningRawContentDelta` is explicitly ignored, or explicitly classified according to the chosen behavior.
* `ItemStarted(WebSearch)` sets `ToolCall` / active work `web_search`.
* `ItemCompleted(WebSearch)` clears tool work and records completion.
* `ItemStarted(ImageGeneration)` sets `ToolCall` / active work `image_generation`.
* `ItemCompleted(ImageGeneration)` clears tool work and records completion.
* `ItemCompleted(AgentMessage)` produces at most one milestone.
* `ItemCompleted(Reasoning)` uses summary text only.
* non-progress `TurnItem` variants are explicit no-ops.

Avoid calling `as_legacy_events(...)` in these tests, as the spec already says. 

### PR 2 — Explicit session observation boundary

This is the highest-risk PR.

Add a compile-time/grep-enforced success condition:

```text
No production call site can call raw event delivery without specifying observation mode.
```

Also add explicit assertions:

* `send_event(ReasoningContentDelta)` increments progress sequence once.
* Generated `AgentReasoningDelta` legacy echo does not increment sequence a second time.
* `send_event(ItemStarted(WebSearch))` increments once.
* Generated `WebSearchBegin` legacy echo does not increment again.
* direct raw `Error` updates progress terminal state.
* direct raw `ShutdownComplete` updates progress terminal state.
* non-progress direct raw event can be observed and ignored without changing sequence.

The core adapter should not inspect `EventMsg` deeply except where already required for existing status/realtime/parent-notification behavior.

### PR 3 — Wait/progress regression tests

The test list is right, but add exact-count checks, not only “wakes.”

Required examples:

* waiting on reasoning wakes from `ReasoningContentDelta`;
* returned reason is `SeqAdvanced` or phase-match according to current wait classifier precedence;
* waiting on tool progress wakes from `ItemStarted(WebSearch)`;
* waiting on tool progress wakes from `ItemStarted(ImageGeneration)`;
* terminal raw `ShutdownComplete` or `Error` does not wait until timeout;
* generated legacy events do not cause a second wake/sequence increment.

Avoid large integration fixtures unless necessary. The useful target is the seam: handler wait loop plus registry sequence.

## Validation changes

The minimum validation in the spec is a little thin. Add:

```text
cargo test -p codex-agent-observability
cargo test -p codex-core agent_progress --lib
cargo check -p codex-core
```

If raw event signatures change broadly, `cargo check -p codex-core` is important because the point of PR2 is to force all raw-delivery call sites through the explicit observation boundary.

## Changes to avoid

Do not restore the old 121 behavior by observing generated legacy events. That would make 0.122 primary events and compatibility echoes compete as semantic sources.

Do not add event-shape case analysis in `codex-core` beyond observation-vs-compatibility routing.

Do not move wait loops or target resolution into `codex-agent-observability`.

Do not touch `codex-tools` for this follow-up unless schema or handler tests expose a direct issue. The original extraction already made tool contract ownership and tool-surface policy explicit. 

## Final judgment

Proceed after amendments.

The spec correctly identifies the 0.122 break: reducer law is still the owner, but the adapter stopped feeding it the right event shape. The main implementation risk is PR2. Make raw observation mode explicit, remove unqualified raw sends, observe primary/runtime events once, mark generated legacy echoes compatibility-only, and preserve rollout persistence separately from progress observation.

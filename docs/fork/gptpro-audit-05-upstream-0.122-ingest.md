## Scope and method

I compared the updated fork against vanilla `0.122.0`, and also checked the relevant custom surfaces against the prior 121-aligned fork state. This is a source-level architecture audit. I did **not** run the Rust test suite because `cargo` is not available in this container.

The two in-scope fork modules are:

1. `codex-context-maintenance-policy`
2. `codex-agent-observability`

That matches the accepted observability boundary: reducer/snapshot/wait/tool-contract law belongs in the observability module, while runtime loops, target resolution, lifecycle orchestration, and tool planning stay adapter-side.  

## Executive judgment

**Context maintenance:** no major 122-ingest regression found. The 121 hardening work appears preserved and improved. The disposition API footgun is fixed, local compact invariant tests exist, and the policy crate remains cleanly isolated.

**Agent observability:** one important likely regression introduced by the 122 ingest/refactor: the progress recording hook moved from the raw delivery boundary to `Session::send_event`, while the reducer still mostly understands legacy event forms. That means several progress-relevant events emitted as 122 structured primary events, or direct `send_event_raw` events, may no longer advance observability state. This is the main thing to fix.

**Contact with vanilla:** the 122 split of old `codex.rs` into `core/src/session/*` is useful. It reduces some hot-file contact for context maintenance, especially around compact timing in `session/turn.rs` and refresh/prune dispatch in `session/handlers.rs`. For observability, it creates a cleaner place to put the event hook, but the current placement is not semantically equivalent to the 121 fork.

## 1. Did the 122 ingest break anything from the 121-aligned fork?

### A. Context maintenance

**Observed:** the custom policy crate boundary survived the 122 ingest.

`context-maintenance-policy/Cargo.toml` still depends only on:

* `codex-protocol`
* `serde`
* `serde_json`
* `thiserror`
* `tracing`

It does **not** depend on `codex-core`, `codex-config`, `codex-tools`, TUI, app-server, or session/runtime crates. That is still the correct dependency direction.

**Observed:** the disposition hardening from the follow-up spec is implemented. The public classifier-less `apply_history_disposition(...)` footgun is gone. The public function now requires a summary classifier, and the no-summary convenience path is test-only. That matches the hardening goal: prevent silent bypass of summary-retention law. 

**Observed:** local compact invariant coverage exists in `core/src/compact_tests.rs`:

* `local_intra_turn_compact_drops_prior_bridge_by_construction`
* `local_turn_boundary_compact_drops_prior_memory_and_bridge_by_construction`

These tests distinguish stale prior artifacts from newly generated authoritative artifacts, check new artifact placement before summary, assert local compaction strips legacy `ResponseItem::Compaction`, and account for ghost snapshot carry-forward. That is the right kind of core-side invariant coverage. It does not restate the whole route matrix.

**Observed:** `core/src/compact.rs` still has the important `LocalPure` override in `should_use_remote_compact_task(turn_context)`. If `CompactionEngine::LocalPure` is configured, it uses local compaction even if the provider supports remote compaction. That was a critical fork behavior, and it appears preserved.

**Observed:** `ContentItem::InputImage` has the upstream `detail` field in 122. The fork’s protocol-level `content_items_to_text` remains robust because it matches `ContentItem::InputImage { .. }`, so this did not reopen the old policy utility leak.

**Judgment:** no context-maintenance break found from the 122 ingest.

Remaining known issue, not caused by 122: `ThreadMemoryGovernance::Disabled` still means “suppress new thread-memory generation,” not “drop/ignore existing thread memory.” That was already deferred. It remains a semantic naming/contract issue, not an ingest regression.

### B. Agent observability

**Observed:** the crate boundary itself is good.

`agent-observability/Cargo.toml` depends on:

* `codex-protocol`
* `serde`
* `tokio`

It does **not** depend on `codex-core`, `codex-tools`, config, TUI, or app-server crates. That matches the intended family invariant: observability law has one semantic owner, while core/tools translate and execute. 

**Observed:** the 122 fork correctly added reducer coverage for `EventMsg::PatchApplyUpdated`. That is good. It classifies patch updates as tool-call activity, treats a first update as a milestone, and subsequent patch updates as heartbeats while refreshing active-work labeling.

**Observed:** the previous lifecycle-schema issue is fixed. `tools/src/agent_progress_tool.rs::agent_status_output_schema()` now includes `"interrupted"`.

**Observed:** `AgentToolSurfacePolicy` now explicitly handles `SubAgentSource::MemoryConsolidation`, hiding collaboration/progress-spawn surface and request-user-input. Prior 121 fork policy did not special-case that source. This is a good tightening.

#### Important regression: event observation moved to the wrong semantic level

In the prior 121-aligned fork, progress recording happened inside raw event delivery:

```rust
deliver_event_raw(event) {
    ...
    agent_control.record_progress_event(self.conversation_id, &event.msg);
    ...
}
```

That meant both primary emitted events and generated legacy events passed through the observability reducer.

In the 122 fork, progress recording now happens in `Session::send_event` before the event is sent:

```rust
let legacy_source = msg.clone();
agent_control.record_progress_event(self.conversation_id, &legacy_source);
self.send_event_raw(event).await;

for legacy in legacy_source.as_legacy_events(...) {
    self.send_event_raw(legacy_event).await;
}
```

But `send_event_raw` and `deliver_event_raw` no longer record progress.

That changes semantics in two ways.

First, generated legacy events are no longer observed. That matters because the reducer still primarily handles legacy-style progress events such as:

* `AgentReasoning`
* `AgentReasoningDelta`
* `AgentMessage`
* `AgentMessageDelta`
* `WebSearchBegin`
* `WebSearchEnd`
* `ImageGenerationBegin`
* `ImageGenerationEnd`

But 122 emits many of these as structured primary events first:

* `ItemStarted(WebSearch)` maps to legacy `WebSearchBegin`
* `ItemCompleted(WebSearch)` maps to legacy `WebSearchEnd`
* `ItemStarted(ImageGeneration)` maps to legacy `ImageGenerationBegin`
* `ItemCompleted(ImageGeneration)` maps to legacy `ImageGenerationEnd`
* `ReasoningContentDelta` maps to legacy `AgentReasoningDelta`
* `ReasoningRawContentDelta` maps to legacy raw-reasoning delta
* `ItemCompleted(AgentMessage)` maps to legacy `AgentMessage`
* `ItemCompleted(Reasoning)` maps to legacy `AgentReasoning`

The reducer currently ignores `ItemStarted`, `ItemCompleted`, `ReasoningContentDelta`, and `ReasoningRawContentDelta`. So after the hook moved, web search, image generation, reasoning deltas, and completed message/reasoning items can stop advancing progress unless they are also emitted elsewhere in legacy form.

Second, direct `send_event_raw` callers are no longer observed. There are many direct raw sends in `session/handlers.rs`, `session/mod.rs`, `session/session.rs`, `turn_context.rs`, realtime code, and network approval code. Many are not progress-relevant. But some are reducer-relevant or wait-relevant, especially `Error`, `Warning`, and `ShutdownComplete`.

Even if `deliver_event_raw` still updates `agent_status`, the progress registry sequence may not advance. That can break `wait_for_agent_progress` behavior because the handler waits on `ProgressRegistry` sequence changes.

**Judgment:** this is a real likely regression relative to the 121-aligned implementation. It is not a crate-boundary failure; it is an adapter placement and reducer coverage failure caused by the 122 event/refactor shape.

## 2. Contact minimization now that vanilla split `codex.rs`

### What improved

Vanilla 122 removed old `core/src/codex.rs` and split it into smaller session modules:

* `core/src/session/mod.rs`
* `core/src/session/turn.rs`
* `core/src/session/handlers.rs`
* `core/src/session/turn_context.rs`
* `core/src/session/session.rs`
* other focused session modules

This improves our seam options.

For context maintenance, the hot contact is now more localized:

* `core/src/session/turn.rs` only needs to translate compact timing into `CompactInvocationTiming`.
* `core/src/session/handlers.rs` only needs refresh/prune operation dispatch.
* `core/src/compact.rs` and `core/src/compact_remote.rs` remain runtime execution adapters.
* `core/src/context_maintenance_runtime.rs` remains the policy adapter.

This is better than having all of it smeared through old `codex.rs`.

For observability, the split creates an obvious event-boundary question:

> Should progress observation attach to `send_event`, `send_event_raw`, or `deliver_event_raw`?

The current answer, `send_event`, is too narrow unless the reducer is updated to structured primary events and direct raw events are intentionally excluded.

### Recommended event boundary after 122

Do **not** blindly restore the old 121 behavior by recording every raw and generated legacy event. That risks double-counting structured primary events and their generated legacy counterparts, especially message deltas.

The better 122-compatible shape is:

1. Make `codex-agent-observability` understand 122 structured primary events.
2. Observe primary events exactly once.
3. Treat generated legacy events as client compatibility output, not reducer input.
4. Make direct raw event sends either explicitly observed or explicitly unobserved.

A concrete adapter shape:

```rust
enum ProgressObservation {
    Observe,
    DoNotObserve,
}

send_event_raw_with_observation(event, ProgressObservation::Observe)
send_legacy_event_raw(event) // DoNotObserve
```

Then:

* `Session::send_event` sends the primary event with `Observe`.
* generated legacy events are sent with `DoNotObserve`.
* direct raw callers use `Observe` only if the event is progress/lifecycle relevant, or a default wrapper observes raw events except known compatibility legacy events.

This avoids both problems: no lost primary structured progress, and no duplicate legacy-derived progress.

## 3. Module-by-module audit after 122

# A. Context-maintenance module

## Strong current points

**Observed:** policy crate still owns:

* route matrix
* maintenance action/timing/engine concepts
* artifact family/lifetime/requiredness
* retention directives
* history disposition
* thread-memory source selection and prompt/schema/parser
* continuation-bridge source selection, subagent supplemental shape, prompt/schema/parser

**Observed:** core remains a runtime adapter:

* model streaming
* session access
* compaction task execution
* app-server/TUI operation dispatch
* config-to-policy translation

This is the intended shape.

## Important remaining work

### CM-1 — Clarify `ThreadMemoryGovernance::Disabled`

Still unresolved.

Current behavior suppresses requested thread-memory generation but does not necessarily drop existing durable `ThreadMemory` or disable thread-memory-gated retention. If that is intended, rename the variant to something like:

```rust
ThreadMemoryGovernance::SuppressGeneration
```

or:

```rust
ThreadMemoryGovernance::DoNotGenerateButHonorExisting
```

If disabled should mean disabled, the overlay should also:

* add `ArtifactKind::ThreadMemory` to drops;
* disable thread-memory-gated retention;
* test prune with existing thread memory under governance-off.

This is not a 122 regression, but it is still the most important semantic ambiguity in context maintenance.

### CM-2 — Reduce remaining core route-doctrine tests later

`core/src/compaction_policy_matrix_tests.rs` still mirrors policy route doctrine. Keep core tests for adapter translation and runtime behavior; keep route law in `context-maintenance-policy`.

This is lower priority than fixing observability event handling.

### CM-3 — Optional contact optimization: shrink `session/mod.rs` prompt-layering changes

The largest remaining session-level custom diff is not strictly context-maintenance policy; it is initial-context / governance prompt-layering section assembly. If upstream keeps changing `session/mod.rs`, consider extracting a narrow helper for “initial context section assembly” or “governance prompt-layering insertion.”

Do **not** put that into `context-maintenance-policy`. It is adjacent runtime/context assembly law, not artifact route law.

# B. Agent-observability module

## Strong current points

**Observed:** `codex-agent-observability` owns:

* progress phases
* blocker reasons
* active work kinds
* snapshots
* registry
* reducer
* wait classification
* progress tool names
* progress schema string constants

This matches the extraction plan’s semantic boundary. 

**Observed:** `codex-tools` owns explicit tool-surface policy via `AgentToolSurfacePolicy`. That is correct. Do not move this into observability.

**Observed:** `codex-core` imports observability only in expected adapter/runtime places:

* `agent/control.rs`
* `thread_manager.rs`
* `tools/handlers/agent_progress.rs`
* `tools/router.rs`
* tests

That is acceptable.

## Required fix

### AO-1 — Teach reducer 122 primary structured events

Add reducer handling for at least:

```rust
EventMsg::ReasoningContentDelta
EventMsg::ReasoningRawContentDelta
EventMsg::ItemStarted
EventMsg::ItemCompleted
```

Suggested classification:

* `ReasoningContentDelta` → same behavior as `AgentReasoningDelta`
* `ReasoningRawContentDelta` → either explicitly ignored or treated like raw reasoning delta; do not leave it under wildcard
* `ItemStarted(TurnItem::WebSearch)` → same as `WebSearchBegin`
* `ItemCompleted(TurnItem::WebSearch)` → same as `WebSearchEnd`
* `ItemStarted(TurnItem::ImageGeneration)` → same as `ImageGenerationBegin`
* `ItemCompleted(TurnItem::ImageGeneration)` → same as `ImageGenerationEnd`
* `ItemCompleted(TurnItem::AgentMessage)` → same final message semantics as `AgentMessage`, but avoid duplicating streaming deltas
* `ItemCompleted(TurnItem::Reasoning)` → same summary semantics as `AgentReasoning`
* other `ItemStarted` / `ItemCompleted` variants → explicitly classified as ignored, pending, or material, depending on intended behavior

Add tests in `agent-observability/src/progress.rs` for these primary event forms. Do not rely on `as_legacy_events` in reducer tests.

### AO-2 — Rework the event hook so direct raw events are not missed

After reducer support for primary events exists, update session event observation so progress-relevant direct raw events still update sequence.

Recommended shape:

* primary `send_event` path observes primary event once;
* generated legacy events are sent without observation;
* direct raw sends either default to observed or must choose observed/unobserved explicitly.

This is especially relevant for `ShutdownComplete`, `Error`, and `Warning`.

### AO-3 — Keep `send_event_raw` from becoming an untyped bypass

The 122 split makes `send_event_raw` a high-risk bypass point. Add a small comment or type-level wrapper so future direct raw call sites make an explicit choice about progress observation.

This is an adapter hardening issue, not a policy crate issue.

## 4. Did 122 give us any new optimization opportunities?

### Opportunity 1 — Use `session/turn.rs` as the compact timing seam

This is already mostly done. The diff in `session/turn.rs` is small and appropriate:

* pass `CompactInvocationTiming::IntraTurn`
* pass `CompactInvocationTiming::TurnBoundary`
* call `should_use_remote_compact_task(turn_context)`

Keep this shape. Do not move compact route law into `session/turn.rs`.

### Opportunity 2 — Keep refresh/prune dispatch in `session/handlers.rs`

The refresh/prune additions in `session/handlers.rs` are small:

* `refresh_context`
* `prune_context`
* `Op::RefreshContext`
* `Op::PruneContext`

This is acceptable vanilla contact. Do not create a broader operation broker just for this.

### Opportunity 3 — Move more observability event semantics out of session

Do not solve the event regression by adding case analysis in `session/mod.rs`. That would reintroduce a second observability law surface.

The reducer should learn structured event meaning. Session should only decide “observe this event once” versus “compatibility legacy event; do not observe.”

### Opportunity 4 — Do not over-optimize protocol contact

The fork still modifies `protocol/src/models.rs` for:

* `content_items_to_text`
* response-item pairing removal utility

This is acceptable because those are protocol-object semantics. Moving them back to context-maintenance policy would be worse.

The fork also modifies `protocol/src/protocol.rs` for refresh/prune operations. That is unavoidable protocol surface, not a modularization smell.

## 5. Upstream-absorption watch plan for future releases

### A. Context-maintenance watchlist

Watch these upstream crates/files first.

#### `protocol/src/models.rs`

Relevant conceptual functions/types:

* `ResponseItem`
* `ContentItem`
* `content_items_to_text`
* `function_call_output_content_items_to_text`
* `remove_corresponding_response_item`
* any new call/output pairing

Review question:

> Does a new response item need to be retained, dropped, paired, excluded from artifact source, or preserved in compacted history?

Custom modules affected:

* `context-maintenance-policy/src/artifact_codecs.rs`
* `context-maintenance-policy/src/thread_memory.rs`
* `context-maintenance-policy/src/continuation_bridge/*`
* `context-maintenance-policy/src/history_shape.rs`

#### `core/src/context_manager/*`

Relevant concepts:

* prompt history exposure
* raw history replacement
* normalization/orphan-output cleanup
* settings update item construction

Review question:

> Did upstream change which items are visible to maintenance artifact generation or how invalid call/output sequences are repaired?

Custom modules affected:

* `core/src/compact.rs`
* `core/src/compact_remote.rs`
* `core/src/context_maintenance.rs`
* `context-maintenance-policy` source selection and disposition code

#### `core/src/session/turn.rs`

Relevant concepts:

* auto-compact triggering
* pre-turn vs mid-turn compact timing
* context-limit compact paths
* model-downshift compact paths

Review question:

> Did upstream add a new compact timing/path that bypasses `runtime_plan_for_compact`?

#### `core/src/compact.rs` and `core/src/compact_remote.rs`

Relevant concepts:

* local compact replacement-history assembly
* remote compact output processing
* summary marker behavior
* provider remote-compaction selection

Review question:

> Does the final compacted history still satisfy the active route plan’s disposition/injection/retention contract?

#### `config/src/config_toml.rs` and `core/src/config/mod.rs`

Relevant concepts:

* compaction engine
* governance path
* continuation bridge config
* thread memory config
* context maintenance model/reasoning settings

Review question:

> Did config shape change require only core adapter translation, or did policy start depending on config schema again?

### B. Agent-observability watchlist

#### `protocol/src/protocol.rs::EventMsg`

Relevant concepts:

* new event variants
* changed event payloads
* structured-to-legacy mappings
* `HasLegacyEvent`
* `ItemStarted`
* `ItemCompleted`
* content delta events

Review question:

> Is this event material progress, heartbeat, blocker, terminal, or intentionally ignored?

This should be a mandatory review gate. The reducer has a wildcard arm, so new events can compile while being semantically ignored.

#### `protocol/src/items.rs::TurnItem`

Relevant concepts:

* new turn item variants
* legacy event mapping for turn items
* web search/image generation/message/reasoning representations

Review question:

> Does a new item type imply progress start/end or final message/reasoning behavior?

#### `core/src/session/mod.rs`

Relevant concepts:

* `send_event`
* `send_event_raw`
* `deliver_event_raw`
* legacy event emission
* agent status updates

Review question:

> Does every progress-relevant event update `ProgressRegistry` exactly once?

#### `core/src/session/handlers.rs`

Relevant concepts:

* direct raw terminal/error/shutdown events
* operation dispatch
* manual compact/refresh/prune handlers

Review question:

> Are direct raw events intentionally observed or intentionally unobserved?

#### `core/src/agent/control.rs`

Relevant concepts:

* `seed_agent_progress`
* `record_progress_event`
* `subscribe_progress_seq`
* `inspect_agent_progress`
* spawn/resume/remove thread paths

Review question:

> Does every agent lifecycle path seed, update, inspect, and remove progress state correctly?

#### `tools/src/tool_config.rs` and `tools/src/tool_registry_plan.rs`

Relevant concepts:

* `AgentToolSurfacePolicy`
* `SessionSource`
* `SubAgentSource`
* collab tools
* spawn-agent availability
* request-user-input availability

Review question:

> Did upstream add a session/subagent source that needs explicit tool-surface policy?

## 6. Recommended PR sequence after this audit

### PR 1 — Fix observability for 122 primary structured events

Add reducer support and tests for:

* `ReasoningContentDelta`
* `ReasoningRawContentDelta`
* `ItemStarted(WebSearch)`
* `ItemCompleted(WebSearch)`
* `ItemStarted(ImageGeneration)`
* `ItemCompleted(ImageGeneration)`
* `ItemCompleted(AgentMessage)`
* `ItemCompleted(Reasoning)`

This is the highest-priority fix.

### PR 2 — Make event observation explicit at the session boundary

Replace the current implicit `send_event`-only progress hook with an explicit observed/unobserved raw-send boundary.

Goal:

* primary event observed once;
* generated legacy events not observed;
* direct raw lifecycle/progress events observed when relevant;
* no silent `send_event_raw` bypass.

### PR 3 — Add regression tests around progress wait

Tests should prove:

* waiting on reasoning progress wakes on `ReasoningContentDelta`;
* waiting on tool-call progress wakes on `ItemStarted(WebSearch)` and `ItemStarted(ImageGeneration)`;
* waiting on terminal/shutdown/error state does not stall until timeout because the event was sent through a raw path.

### PR 4 — Leave context maintenance mostly alone

No large context-maintenance refactor is needed after 122.

Only do narrow follow-ups:

* clarify or rename `ThreadMemoryGovernance::Disabled`;
* reduce duplicated core route-doctrine tests later;
* optionally extract initial-context section assembly from `session/mod.rs` if upstream churn makes that file painful.

## Final assessment

The 122 ingest preserved the context-maintenance refactor well. The policy crate boundary is still correct, and the disposition/local-compact hardening appears implemented.

The observability crate boundary is also still correct, but the event adapter is no longer equivalent to the 121-aligned fork. The old hook observed all raw-delivered events, including generated legacy events. The new hook observes only the primary `send_event` input and misses generated legacy events plus direct raw sends. Because the reducer still depends heavily on legacy event variants, 122 likely regressed progress detection for reasoning, web search, image generation, and some terminal/raw event paths.

The right fix is not to move law back into core. The right fix is to update `codex-agent-observability` to understand 122 primary structured events and make the session event observation boundary explicit.

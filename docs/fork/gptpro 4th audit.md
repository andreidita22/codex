I inspected the updated fork against vanilla `codex-rust-v0.121.0`. I did not run the Rust test suite because `cargo` is not available in the container, so this is a source-level architecture audit, not a compile/test result.

The two refactored custom families I’m treating as in-scope are:

1. **Context maintenance** via `codex-context-maintenance-policy`.
2. **Agent observability / E-witness** via `codex-agent-observability` plus explicit tool-surface policy in `codex-tools`.

That matches the observability plan’s intended scope: reducer/registry/wait/tool-contract ownership in the observability crate, with tool-surface containment kept in `codex-tools`, not a broad `AgentControl` or collaboration extraction.  The implementation spec’s family invariant is also now substantially reflected in code: semantic progress law is module-owned, while runtime/session/tool-registration code remains adapter-side. 

## 1. Audit of both refactored custom features

# A. Context maintenance

## Current judgment

The context-maintenance refactor is now structurally strong. The policy crate is a real semantic boundary, not just moved code.

Observed crate boundary:

* `context-maintenance-policy` depends on:

  * `codex-protocol`
  * `serde`
  * `serde_json`
  * `thiserror`
  * `tracing`
* It does **not** depend on:

  * `codex-core`
  * `codex-config`
  * `codex-tools`
  * TUI/app-server/runtime crates

That is the right dependency shape.

The previous major leaks have mostly been closed:

* `codex-config` is no longer imported by the policy crate.
* `content_items_to_text` moved to `codex-protocol::models`, which is the correct owner.
* response-item call/output pairing cleanup moved to `codex-protocol::models::remove_corresponding_response_item`.
* artifact requiredness survives through `RuntimeMaintenancePlan`.
* compact timing is explicit via `CompactInvocationTiming`, no longer inferred from injection placement.
* history disposition is policy-owned through `HistoryDispositionPolicy`.
* remote compact shaping is policy-owned through `shape_remote_compacted_history`.
* thread-memory and continuation-bridge source selection are policy-owned.
* legacy compaction-marker stripping is executed by `apply_history_disposition*`.

## What is already good

### Route law is now centralized

`context-maintenance-policy/src/route_matrix.rs` owns:

* action/timing/engine route combinations;
* context injection policy;
* requested artifacts;
* artifact lifetime;
* artifact requiredness;
* history disposition;
* retention directive;
* governance overlay effects.

That is the correct semantic owner.

### Runtime adapter is mostly clean

`core/src/context_maintenance_runtime.rs` now acts like a true adapter:

* maps core config engine into `PolicyEngine`;
* maps governance path into `ThreadMemoryGovernance`;
* constructs runtime plans from policy plans;
* centralizes required vs best-effort artifact generation in `execute_requested_artifact`.

Core still owns model streaming, session state, and task execution. That is correct.

### Protocol-generic helpers are no longer policy-owned

The earlier problem where the policy crate became a generic `ContentItem` text utility provider is fixed. `content_items_to_text` and response-item pairing belong to `codex-protocol`, which is the right owner because those functions describe protocol objects, not context-maintenance law.

## Important remaining context-maintenance work

### CM-1 — Clarify `ThreadMemoryGovernance::Disabled`

**Observed:** `ThreadMemoryGovernance::Disabled` currently suppresses **new thread-memory artifact requests**, but it does not necessarily mean “drop existing durable thread memory” or “ignore existing thread memory for retention.”

In `route_matrix.rs`, the governance overlay removes requested `ArtifactKind::ThreadMemory` artifacts and records `GovernanceEffect::ThreadMemorySuppressed`. It does not change:

* `history_disposition.drop_prior_artifact_kinds`;
* `retention_directive`;
* existing durable thread-memory artifacts.

This matters most for prune. The prune route drops `PruneManifest` and `ContinuationBridge`, but not `ThreadMemory`, and it still uses retention gated on final history containing `ThreadMemory`.

**Risk:** semantic ambiguity. The enum name sounds like “thread memory disabled,” but behavior is closer to “do not generate a new thread-memory artifact.”

**Recommendation:** make the meaning explicit.

Option A, if current behavior is intended:

```rust
ThreadMemoryGovernance::SuppressGeneration
```

or:

```rust
ThreadMemoryGovernance::DoNotGenerateButHonorExisting
```

Option B, if “disabled” should mean disabled:

* add `ArtifactKind::ThreadMemory` to drops under the disabled overlay;
* disable thread-memory-gated retention;
* test prune with existing durable thread memory and governance off.

**Priority:** high. This is the main remaining semantic ambiguity.

---

### CM-2 — `apply_history_disposition` has a public footgun

**Observed:** `apply_history_disposition` is public and has only a `debug_assert!` preventing use with `SummaryDispositionPolicy::KeepLatestCompactionSummary`. In release builds, that call would proceed with a classifier that always returns false.

Current production paths appear to avoid this misuse: prune uses `apply_history_disposition_with_summary_classifier`. But the public API permits incorrect usage.

**Risk:** future caller can accidentally bypass summary-retention semantics.

**Recommendation:** harden one of these ways:

* make `apply_history_disposition` private or `pub(crate)` if external callers do not need it;
* return a `Result` if the policy requires a classifier;
* split the request type so `KeepLatestCompactionSummary` cannot be passed to the classifier-less function.

**Priority:** medium-high. Small change, good boundary hardening.

---

### CM-3 — Local compact does not execute final-history disposition through the common helper

**Observed:** remote compact and refresh/prune now use policy-owned history disposition directly. Local compact still builds the replacement history by construction in `core/src/compact.rs`.

That may be behaviorally correct, but it means the route matrix contains disposition law that local compact does not visibly execute through the same path.

**Risk:** future route changes can update policy disposition but not local compact assembly.

**Recommendation:** do not rewrite local compact. Add explicit invariant tests instead:

* local intra-turn compact satisfies its route’s artifact-drop/marker policy by construction;
* local turn-boundary compact satisfies thread-memory/bridge drop expectations;
* local compact does not retain prior continuation bridges in replacement history;
* local compact strips or preserves compaction markers according to policy.

If safe, apply `apply_history_disposition` to local replacement history as a final deterministic pass. But only do that after snapshot verification.

**Priority:** medium.

---

### CM-4 — `ArtifactKind::CompactionMarker` remains slightly misleading

**Observed:** `ArtifactKind::CompactionMarker` is public, but legacy compaction markers are stripped via `LegacyCompactionMarkerPolicy`, not by tagged-artifact removal. A future caller may reasonably assume that dropping `ArtifactKind::CompactionMarker` removes `ResponseItem::Compaction`; that is not how current disposition works.

**Recommendation:** either:

* remove `CompactionMarker` from public artifact-drop paths; or
* make `apply_history_disposition` explicitly handle `ArtifactKind::CompactionMarker` inside `drop_prior_artifact_kinds`; or
* document it as non-droppable marker classification and keep all marker behavior under `LegacyCompactionMarkerPolicy`.

**Priority:** medium-low, unless more callers start manipulating artifact kinds directly.

---

### CM-5 — Test doctrine still exists in core

**Observed:** `core/src/compaction_policy_matrix_tests.rs` still mirrors significant route doctrine now owned by the policy crate.

**Risk:** two law surfaces in tests. Future route changes may need edits in both places, and core tests may lock adapter presentation instead of policy behavior.

**Recommendation:** eventually reduce core matrix tests to adapter coverage:

* config engine → policy engine;
* governance config → policy governance;
* compact timing → policy timing;
* unsupported route propagation;
* runtime required/best-effort artifact handling.

Keep route doctrine in `context-maintenance-policy`.

**Priority:** low-medium. Do after semantic issues above.

## Context-maintenance conclusion

No broad redesign needed. The refactor succeeded. Remaining work is mostly semantic tightening:

1. decide/rename governance-disabled semantics;
2. harden disposition API;
3. add local compact disposition invariants;
4. reduce public/API/test footguns.

# B. Agent observability / E-witness

## Current judgment

The observability extraction is also fundamentally successful.

Observed crate boundary:

* `agent-observability` depends on:

  * `codex-protocol`
  * `serde`
  * `tokio`
* It does **not** depend on:

  * `codex-core`
  * `codex-tools`
  * config/UI/runtime crates

That is the correct shape.

The accepted audit target was to move reducer law, progress snapshots, sequence rules, wait classification, and model-visible progress tool contract out of `codex-core`, while keeping runtime orchestration and tool planning in their existing crates.  That is mostly done.

## What is already good

### Reducer law moved out of core

`agent-observability/src/progress.rs` now owns:

* `AgentProgressPhase`;
* `AgentBlockReason`;
* `AgentActiveWorkKind`;
* `AgentActiveWork`;
* `AgentProgressSnapshot`;
* `ProgressRegistry`;
* event-to-progress reduction;
* sequence advancement;
* stall calculation;
* lifecycle-status-to-progress interpretation.

That is the correct owner.

### Wait semantics moved into the crate

`agent-observability/src/wait.rs` owns:

* `WaitObservationMoment`;
* `WaitForAgentProgressMatchReason`;
* `classify_wait_observation`.

The important precedence rule is implemented: sequence advancement wins before phase matching. Initial phase match returns `AlreadySatisfied`; later phase match returns `PhaseMatched`.

That matches the hardened version of the spec.

### Tool contract strings are mostly single-sourced

`agent-observability/src/contract.rs` owns:

* `INSPECT_AGENT_PROGRESS_TOOL_NAME`;
* `WAIT_FOR_AGENT_PROGRESS_TOOL_NAME`;
* progress phase names;
* blocker reason names;
* active-work kind names;
* wait match reason names.

`codex-tools` consumes those constants in `agent_progress_tool.rs` and `tool_registry_plan.rs`. `codex-core` consumes the same constants in `tools/router.rs`.

This closes the original drift risk around tool names and phase schema.

### Thread-spawn containment is explicit in `codex-tools`

`tools/src/tool_config.rs` now has `AgentToolSurfacePolicy`.

That is the right owner. It encodes:

* collaboration/progress tool availability;
* recursive `spawn_agent` suppression for thread-spawned subagents;
* `request_user_input` suppression for subagents.

This matches the intended split: observability crate owns progress law; `codex-tools` owns tool-surface planning.

## Important remaining observability work

### AO-1 — `AgentStatus` output schema is missing `interrupted`

**Observed:** `protocol/src/protocol.rs::AgentStatus` includes:

```rust
PendingInit
Running
Interrupted
Completed(Option<String>)
Errored(String)
Shutdown
NotFound
```

But `tools/src/agent_progress_tool.rs::agent_status_output_schema()` lists only:

```json
["pending_init", "running", "shutdown", "not_found"]
```

for string statuses. It omits:

```json
"interrupted"
```

Meanwhile `agent-observability/src/progress.rs` explicitly maps `AgentStatus::Interrupted` to `AgentProgressPhase::Interrupted`.

**Risk:** concrete model-visible schema bug. The handler can output a lifecycle status that the tool schema does not allow.

**Recommendation:** fix immediately.

Minimal fix:

* add `"interrupted"` to `agent_status_output_schema`;
* add a test that serializes all simple `AgentStatus` variants and verifies the schema/constant list includes them.

Better fix:

* move lifecycle-status schema generation closer to `codex-protocol`, or generate it from `schemars::schema_for!(AgentStatus)` if practical.

**Priority:** highest. This is the one concrete correctness issue I found.

---

### AO-2 — New upstream `EventMsg` variants can be silently ignored

**Observed:** `ProgressRegistry::record_event` has a final `_ => {}` arm.

That is reasonable for current behavior because not every event should affect progress. But it means future upstream `EventMsg` additions will compile without forcing a review.

**Risk:** upstream adds a new meaningful event type, and observability silently ignores it.

**Recommendation:** keep the wildcard if needed, but add an upstream-ingest review gate:

* every new `EventMsg` variant must be classified as:

  * material progress;
  * heartbeat/non-material;
  * blocker;
  * terminal;
  * intentionally ignored.

Optionally add a test fixture listing known handled material/blocking/terminal events so review has an obvious place to update.

**Priority:** medium-high for ingestion discipline; not necessarily a code change now.

---

### AO-3 — Lifecycle finality split should be documented/tested

**Observed:** observability treats `AgentStatus::Interrupted` as final for progress/stall purposes. Core lifecycle code may treat interrupted differently because interrupted agents can potentially receive more input.

That split can be valid:

* lifecycle finality: “can this agent still be resumed or addressed?”
* progress finality: “should this snapshot be considered actively running/stalled?”

**Risk:** future maintainers may “deduplicate” the two concepts and break either wait behavior or lifecycle behavior.

**Recommendation:** add a small test/comment documenting that observability finality is progress-finality, not thread-lifecycle finality.

**Priority:** medium.

---

### AO-4 — Core tests still carry some tool-planning doctrine

**Observed:** `core/src/tools/spec_tests.rs` still carries expected tool-list doctrine that overlaps with `codex-tools` planning tests.

**Risk:** duplicate doctrine. Less severe now that `AgentToolSurfacePolicy` exists, but still noisy during upstream ingestion.

**Recommendation:** keep core tests for:

* handler kind registration;
* router/code-mode behavior;
* runtime handler wiring.

Move tool-surface matrix doctrine to:

* `tools/src/tool_config_tests.rs`;
* `tools/src/tool_registry_plan_tests.rs`.

**Priority:** low-medium.

---

### AO-5 — `TimedOut` as a “match reason” is semantically awkward

**Observed:** `WaitForAgentProgressMatchReason` includes `TimedOut`.

That preserves current output shape, but timeout is not really a match reason. It is an outcome reason.

**Recommendation:** do not churn this now unless you are already changing the API. If touched later, split internally:

```rust
WaitMatchReason::{AlreadySatisfied, SeqAdvanced, PhaseMatched}
WaitOutcomeReason::{Matched(WaitMatchReason), TimedOut}
```

Keep serialized output stable unless there is a deliberate protocol break.

**Priority:** low.

## Observability conclusion

The observability refactor is successful. The only urgent fix is the missing `interrupted` lifecycle status in the progress tool output schema. Everything else is ingestion hardening or test cleanup.

# 2. Upstream vanilla absorption plan

The goal during upstream absorption is not to “re-audit everything.” The goal is to identify upstream conceptual changes that can invalidate our two extracted semantic modules or their adapters.

The release process should compare:

```text
old vanilla -> new vanilla
```

first, not only:

```text
our fork -> new vanilla
```

For every upstream change in the watchlist below, classify it as one of:

* protocol shape change;
* history/context-manager shape change;
* compact/runtime path change;
* model-client/streaming change;
* event/lifecycle change;
* tool-planning change;
* config/app-server surface change.

Then inspect the corresponding custom crate and adapter.

## A. Context-maintenance upstream watchlist

### 1. `codex-protocol`

Watch:

* `protocol/src/models.rs`

  * `ResponseItem`
  * `ContentItem`
  * `FunctionCallOutputPayload`
  * `content_items_to_text`
  * `function_call_output_content_items_to_text`
  * `remove_corresponding_response_item`

Why it matters:

* context-maintenance artifacts are represented as `ResponseItem`s;
* source selection filters over `ResponseItem`;
* remote history shaping filters over `ResponseItem`;
* thread-memory trimming depends on call/output pairing.

Mandatory review trigger:

* any new `ResponseItem` variant;
* any new call/output pair;
* any changed assistant/user/developer message representation;
* any changed content-item text representation.

Review custom locations:

* `context-maintenance-policy/src/artifact_codecs.rs`
* `context-maintenance-policy/src/thread_memory.rs`
* `context-maintenance-policy/src/continuation_bridge/mod.rs`
* `context-maintenance-policy/src/history_shape.rs`
* `core/src/context_manager/normalize.rs`

Key question:

> Should this new protocol item be retained, dropped, paired, summarized, or excluded from maintenance artifact source input?

---

### 2. `core/src/context_manager`

Watch:

* `core/src/context_manager/history.rs`

  * `record_items`
  * `for_prompt`
  * `raw_items`
  * `remove_first_item`
  * history replacement behavior
* `core/src/context_manager/normalize.rs`

  * call-output normalization
  * orphan-output removal
  * image stripping
  * any response-item pairing logic
* `core/src/context_manager/updates.rs`

Why it matters:

* policy artifacts are generated from prompt/history items;
* final history replacement must not leave orphaned or invalid protocol sequences;
* source-selection assumptions depend on what `for_prompt` exposes.

Review custom locations:

* `core/src/compact.rs`
* `core/src/compact_remote.rs`
* `core/src/context_maintenance.rs`
* `context-maintenance-policy/src/thread_memory.rs`
* `context-maintenance-policy/src/continuation_bridge/mod.rs`

Key question:

> Did upstream change which history items are visible to compact/artifact generation, or how invalid call/output sequences are repaired?

---

### 3. Local compact path

Watch:

* `core/src/compact.rs`

  * `run_inline_auto_compact_task`
  * `run_compact_task`
  * `run_compact_task_inner`
  * `run_compact_task_inner_impl`
  * `should_use_remote_compact_task`
  * `InitialContextInjection`
  * `build_compacted_history`
  * `collect_user_messages`
  * `is_raw_conversation_message`
  * `is_real_user_message`
  * `is_user_or_summary_message`
  * `is_summary_message`

Why it matters:

* local compact is still the path most likely to bypass shared disposition by construction;
* injection and source-history timing are core adapter responsibilities;
* summary-shaping changes can invalidate thread-memory source filters.

Review custom locations:

* `core/src/context_maintenance_runtime.rs`
* `context-maintenance-policy/src/route_matrix.rs`
* `context-maintenance-policy/src/retention.rs`
* `context-maintenance-policy/src/history_shape.rs`

Key question:

> Did upstream introduce a new compact timing, source-history shape, summary marker, or replacement-history assembly rule?

---

### 4. Remote compact path

Watch:

* `core/src/compact_remote.rs`

  * `run_inline_remote_auto_compact_task`
  * `run_remote_compact_task`
  * `run_remote_compact_task_inner_impl`
  * `process_compacted_history`
  * `should_keep_compacted_history_user_message`
  * `is_compaction_summary_message`
  * `trim_function_call_history_to_fit_context_window`
  * model-client compact call handling

Why it matters:

* remote compact output is shaped by policy but classified by core predicates;
* upstream remote compact changes can alter insertion point, marker preservation, developer-message retention, or summary handling.

Review custom locations:

* `context-maintenance-policy/src/history_shape.rs`
* `context-maintenance-policy/src/artifact_codecs.rs`
* `core/src/context_maintenance_runtime.rs`

Key question:

> Does the policy-owned remote history shape still match upstream remote compact semantics?

---

### 5. `core/src/codex.rs`

Watch:

* turn submission and operation dispatch;
* compact/refresh/prune op handling;
* `run_pre_sampling_compact`;
* `maybe_run_previous_model_inline_compact`;
* `run_auto_compact`;
* `build_initial_context`;
* `build_prompt`;
* `replace_history`;
* `replace_compacted_history`;
* `clone_history`;
* `reference_context_item`;
* `build_settings_update_items`;
* `TurnContext` construction and fields.

Why it matters:

* this is the hot runtime seam where compact timing and history replacement are triggered;
* upstream can add new pre-sampling, post-tool, or prompt-building behavior that bypasses the policy adapter.

Review custom locations:

* `core/src/context_maintenance_runtime.rs`
* `core/src/context_maintenance.rs`
* `core/src/compact.rs`
* `core/src/compact_remote.rs`

Key question:

> Did upstream create a new context-maintenance-like path that does not call the policy adapter?

---

### 6. Event mapping and turn-item parsing

Watch:

* `core/src/event_mapping.rs`

  * `parse_turn_item`
  * user/developer contextual message detection
  * hook/contextual message classification

Why it matters:

* policy history shaping depends on core-supplied classifiers;
* remote compact keep/drop law uses these classifiers.

Review custom locations:

* `core/src/compact_remote.rs`
* `context-maintenance-policy/src/history_shape.rs`
* `context-maintenance-policy/src/retention.rs`

Key question:

> Did upstream change what counts as a real user message, contextual user message, hook message, or summary-like message?

---

### 7. Config surfaces

Watch:

* `config/src/config_toml.rs`
* `core/src/config/mod.rs`
* `core/config.schema.json`
* generated config schema/tests

Concepts:

* compaction engine;
* continuation bridge config;
* context-maintenance model/reasoning settings;
* governance path;
* thread memory mode;
* default values.

Why it matters:

* policy crate must remain decoupled from config/TOML;
* core adapter must continue translating config into policy-owned types.

Review custom locations:

* `core/src/context_maintenance_config.rs`
* `core/src/context_maintenance_runtime.rs`
* `context-maintenance-policy/src/contracts.rs`

Key question:

> Did a config enum/default change require only adapter changes, or did policy accidentally become config-shaped again?

---

### 8. Protocol/app-server operation surfaces

Watch:

* `protocol/src/protocol.rs`

  * `Op`
  * `Op::kind`
  * `ThreadMemoryMode`
* `app-server-protocol/src/protocol/common.rs`
* `app-server-protocol/src/protocol/v2.rs`
* generated JSON/TypeScript schema
* `app-server/src/codex_message_processor.rs`

Why it matters:

* refresh/prune/thread-memory operations have protocol surfaces;
* app-server request/response mapping can drift.

Review custom locations:

* `core/src/context_maintenance.rs`
* `core/src/tasks/context_maintenance.rs`
* app-server tests around refresh/prune.

Key question:

> Did upstream alter request/operation dispatch in a way that bypasses refresh/prune maintenance tasks?

---

### 9. Model-client and task execution

Watch:

* `core/src/client.rs`
* model streaming APIs;
* `compact_conversation_history`;
* response-format/schema plumbing;
* reasoning/model request configuration;
* `core/src/tasks/compact.rs`
* `core/src/tasks/mod.rs`

Why it matters:

* continuation bridge and thread memory generation use model-client streaming and schema/result parsing;
* remote compact depends on upstream model-client compact APIs.

Review custom locations:

* `core/src/continuation_bridge.rs`
* `core/src/governance/thread_memory.rs`
* `core/src/context_maintenance_runtime.rs`
* `context-maintenance-policy/src/continuation_bridge/mod.rs`
* `context-maintenance-policy/src/thread_memory.rs`

Key question:

> Did model streaming or response-format handling change such that artifact generation needs adapter updates?

## B. Agent-observability upstream watchlist

### 1. `codex-protocol::protocol::EventMsg`

Watch:

* `protocol/src/protocol.rs`

  * `EventMsg`
  * event structs for:

    * `TurnStarted`
    * `AgentReasoning`
    * `AgentReasoningDelta`
    * `AgentMessage`
    * `AgentMessageDelta`
    * `AgentMessageContentDelta`
    * `ExecCommandBegin`
    * `ExecCommandOutputDelta`
    * `TerminalInteraction`
    * `ExecCommandEnd`
    * `McpToolCallBegin`
    * `McpToolCallEnd`
    * `DynamicToolCallRequest`
    * `DynamicToolCallResponse`
    * `WebSearchBegin`
    * `WebSearchEnd`
    * `ImageGenerationBegin`
    * `ImageGenerationEnd`
    * `PatchApplyBegin`
    * `PatchApplyEnd`
    * `ExecApprovalRequest`
    * `ApplyPatchApprovalRequest`
    * `RequestPermissions`
    * `RequestUserInput`
    * `ElicitationRequest`
    * `Warning`
    * `StreamError`
    * `TurnComplete`
    * `Error`
    * `TurnAborted`
    * `ShutdownComplete`

Why it matters:

* observability reducer law is a classification of `EventMsg`;
* new event variants can be silently ignored due to wildcard matching.

Review custom locations:

* `agent-observability/src/progress.rs`
* `agent-observability/src/contract.rs`
* `agent-observability/src/wait.rs`

Key question:

> Is the new/changed event material progress, heartbeat, blocker, terminal, or intentionally ignored?

---

### 2. `AgentStatus` and lifecycle status derivation

Watch:

* `protocol/src/protocol.rs::AgentStatus`
* `core/src/agent/status.rs`

  * `agent_status_from_event`
  * `is_final`
* any new lifecycle events or statuses.

Why it matters:

* observability snapshots output `AgentStatus`;
* progress phase can be overridden by lifecycle status;
* tool output schema must match protocol status serialization.

Review custom locations:

* `agent-observability/src/progress.rs`

  * `phase_for_status`
  * `is_final_for_progress`
* `tools/src/agent_progress_tool.rs`

  * lifecycle-status output schema.

Key question:

> Does every serialized lifecycle status have a valid tool output schema and a deliberate progress interpretation?

Immediate current fix from this audit:

* add `"interrupted"` to the progress tool lifecycle-status schema.

---

### 3. Thread/session source concepts

Watch:

* `protocol/src/protocol.rs`

  * `SessionSource`
  * `SubAgentSource`
  * `AgentPath`
  * `InterAgentCommunication`
  * `ThreadId`

Why it matters:

* `AgentToolSurfacePolicy` derives spawn/request-user-input availability from session source;
* target resolution and thread-spawn containment depend on these shapes.

Review custom locations:

* `tools/src/tool_config.rs`
* `tools/src/tool_registry_plan.rs`
* `core/src/agent/control.rs`
* `core/src/tools/handlers/agent_progress.rs`

Key question:

> Did upstream add a new session source or subagent source that needs explicit tool-surface policy?

---

### 4. Agent runtime control

Watch:

* `core/src/agent/control.rs`

  * `spawn_agent_internal`
  * `spawn_agent_with_metadata`
  * `resume_agent`
  * `seed_agent_progress`
  * `record_progress_event`
  * `subscribe_progress_seq`
  * `inspect_agent_progress`
  * `get_status`
  * `get_agent_metadata`
  * `open_thread_spawn_children`

Why it matters:

* this is the adapter between live runtime and observability crate;
* upstream changes to spawn/resume/removal can create stale registry entries or missed seed events.

Review custom locations:

* `agent-observability/src/progress.rs`
* `core/src/thread_manager.rs`
* `core/src/tools/handlers/agent_progress.rs`

Key question:

> Does every agent lifecycle path seed, update, inspect, and remove progress state correctly?

---

### 5. Event delivery path

Watch:

* `core/src/codex.rs`

  * `deliver_event_raw`
  * `send_event_raw`
  * agent status update call
  * progress recording hook
  * parent/child terminal notification paths.

Why it matters:

* observability depends on seeing emitted events;
* upstream may split event delivery, buffer events, or introduce a new path that bypasses the hook.

Review custom locations:

* `core/src/agent/control.rs`
* `agent-observability/src/progress.rs`

Key question:

> Does every emitted event that should affect progress still pass through `record_progress_event` exactly once?

---

### 6. Thread manager storage/lifetime

Watch:

* `core/src/thread_manager.rs`

  * `ThreadManagerState`
  * registry initialization
  * thread removal
  * resume/restore path.

Why it matters:

* `ProgressRegistry` is stored in thread-manager state;
* upstream lifecycle changes can leak or drop progress state incorrectly.

Review custom locations:

* `agent-observability/src/progress.rs`
* `core/src/agent/control.rs`

Key question:

> Does registry lifetime still match thread lifetime?

---

### 7. Progress tool handlers

Watch:

* `core/src/tools/handlers/agent_progress.rs`
* `core/src/tools/handlers/multi_agents_common.rs`
* `core/src/tools/handlers/multi_agents/*`
* target resolution helpers.

Why it matters:

* wait loops remain runtime-side;
* target resolution is deliberately not owned by the observability crate;
* upstream may change target identity, agent paths, or timeout conventions.

Review custom locations:

* `agent-observability/src/wait.rs`
* `agent-observability/src/progress.rs`

Key question:

> Are handler wait loops still only executing runtime waiting, not re-authoring wait-match law?

---

### 8. Tool planning and registry

Watch:

* `tools/src/tool_config.rs`

  * `ToolsConfig::new`
  * `AgentToolSurfacePolicy`
* `tools/src/tool_registry_plan.rs`
* `tools/src/tool_registry_plan_types.rs`

  * `ToolHandlerKind`
* `tools/src/agent_progress_tool.rs`
* `tools/src/tool_config_tests.rs`
* `tools/src/tool_registry_plan_tests.rs`

Why it matters:

* progress tool names and schemas must stay single-sourced from `codex-agent-observability`;
* recursive spawn containment must stay explicit in `codex-tools`.

Review custom locations:

* `agent-observability/src/contract.rs`
* `core/src/tools/spec.rs`
* `core/src/tools/router.rs`

Key question:

> Did upstream add or rename a tool-surface flag that should be represented in `AgentToolSurfacePolicy`?

---

### 9. Core tool routing/spec wiring

Watch:

* `core/src/tools/spec.rs`
* `core/src/tools/router.rs`
* `core/src/tools/spec_tests.rs`

Why it matters:

* core should only wire handler kinds and routing;
* tool contract strings should continue to come from observability constants.

Review custom locations:

* `agent-observability/src/contract.rs`
* `tools/src/agent_progress_tool.rs`

Key question:

> Did core reintroduce hardcoded progress tool names or schema doctrine?

## C. Incoming-release procedure

Use this as the merge checklist.

### Step 1 — Diff upstream old vanilla to new vanilla

Before touching the fork, inspect upstream changes in watched crates.

Useful categories:

```text
protocol-shape
history-shape
compact-runtime
event-lifecycle
agent-runtime
tool-planning
config-surface
app-server-surface
model-client
```

Any change in those categories requires custom-feature review.

### Step 2 — Review enum changes first

Mandatory manual review for:

* new `ResponseItem` variants;
* changed `ContentItem` shape;
* new response call/output pair;
* new `EventMsg` variants;
* new `AgentStatus` variants;
* new `SessionSource` or `SubAgentSource` variants;
* new `ToolHandlerKind`;
* new compact/config/protocol operation.

These are the highest-risk upstream changes because they can compile while silently changing semantic behavior.

### Step 3 — Re-run semantic crate tests first

Run these before broad integration tests:

```text
codex-context-maintenance-policy tests
codex-agent-observability tests
codex-tools tool_config/tool_registry_plan tests
```

If these fail, fix doctrine at the owning crate before adapting runtime.

### Step 4 — Then run adapter/runtime tests

High-signal areas:

```text
core compact tests
core compact_remote tests
core compact_resume_fork tests
core context-maintenance tests
core agent_progress handler tests
core multi-agent handler tests
app-server refresh/prune/context tests
```

Snapshot changes around compaction must be reviewed manually. Do not auto-accept them.

### Step 5 — Check for new bypass paths

For each upstream release, explicitly ask:

Context maintenance:

* Is there a new compact path that bypasses `runtime_plan_for_compact`?
* Is there a new refresh/prune-like operation that bypasses `runtime_plan_for_turn_boundary_maintenance`?
* Is there a new history replacement path that bypasses policy disposition?
* Is there a new prompt-building path that changes artifact source visibility?

Agent observability:

* Is there a new event delivery path that bypasses `record_progress_event`?
* Is there a new agent lifecycle path that skips seed/remove?
* Is there a new session/source type that bypasses `AgentToolSurfacePolicy`?
* Is there a new progress-visible event ignored by the reducer?

### Step 6 — Watch for ownership regressions

Reject or refactor merges that reintroduce these patterns:

Context maintenance:

* `context-maintenance-policy` imports `codex-core` or `codex-config`.
* core reimplements route law instead of consuming `MaintenancePolicyPlan`.
* prompts/schemas/tags for thread memory or continuation bridge move back into core.
* `ResponseItem` generic utility logic moves back into policy.

Agent observability:

* `agent-observability` imports `codex-core` or `codex-tools`.
* core reimplements phase transitions or wait-match law.
* tool names or phase strings are hardcoded again in core/tools.
* thread-spawn containment becomes loose booleans again instead of `AgentToolSurfacePolicy`.

## D. Practical grep gates for upstream releases

Use these on the new vanilla release and compare against old vanilla.

### Context-maintenance grep gate

```bash
rg -n "enum ResponseItem|enum ContentItem|content_items_to_text|remove_corresponding|FunctionCallOutputPayload" protocol/src

rg -n "compact_conversation_history|run_auto_compact|run_pre_sampling_compact|process_compacted_history|replace_compacted_history|build_initial_context|build_prompt" core/src

rg -n "for_prompt|record_items|raw_items|normalize_history|remove_orphan|call_output" core/src/context_manager

rg -n "parse_turn_item|contextual.*message|compaction summary|summary message" core/src

rg -n "RefreshContext|PruneContext|ThreadMemoryMode|Op::Compact|Op::Refresh|Op::Prune" protocol/src core/src app-server* config/src
```

### Agent-observability grep gate

```bash
rg -n "enum EventMsg|enum AgentStatus|enum SessionSource|enum SubAgentSource|ThreadId|TurnAbortReason" protocol/src

rg -n "deliver_event_raw|send_event_raw|agent_status_from_event|maybe_notify_parent|forward_child_completion" core/src

rg -n "seed_agent_progress|record_progress_event|subscribe_progress_seq|inspect_agent_progress|get_status|spawn_agent|resume_agent" core/src/agent core/src/thread_manager.rs

rg -n "ToolHandlerKind|ToolsConfig::new|AgentToolSurfacePolicy|tool_registry|model_visible_in_code_mode_only" tools/src core/src/tools

rg -n "inspect_agent_progress|wait_for_agent_progress|AgentProgressPhase|WaitForAgentProgress" .
```

## Final prioritized action list

### Do now

1. **Fix `AgentStatus::Interrupted` in the progress tool output schema.**
2. **Decide and encode `ThreadMemoryGovernance::Disabled` semantics.**

   * Rename if it only suppresses generation.
   * Change overlay if it should drop/ignore existing memory.

### Do next

3. Harden `apply_history_disposition` so summary disposition cannot be silently misused.
4. Add local compact disposition invariant tests.
5. Document/test the lifecycle-finality split for interrupted agents.

### Do later

6. Reduce duplicate core route/tool doctrine tests.
7. Narrow public APIs in both semantic crates where production adapters no longer need low-level helpers.
8. Add formal upstream-ingest review gates for new `ResponseItem` and `EventMsg` variants.

Net assessment: both refactors are architecturally successful. The remaining issues are narrow. The only immediate correctness bug I saw is the missing `interrupted` lifecycle status in the observability tool schema; the main remaining policy ambiguity is the meaning of context-maintenance governance-off behavior for existing thread memory.

# Upstream Ingest Watchlist

Use this document during every upstream vanilla release ingest together with:

- [fork-updates.md](fork-updates.md)
- [fork-update-checklist.md](fork-update-checklist.md)
- [custom-fork-module-inventory.md](custom-fork-module-inventory.md)
- [release-alignment-log.md](release-alignment-log.md)

This document answers a different question from the branch workflow docs:

- which upstream conceptual changes can silently break our extracted modules
- which crates and functions must be reviewed before merging a new vanilla
  release into fork `main`
- which grep gates should be run on the old and new upstream baselines before
  touching fork code

## Release review gates

For every upstream release:

1. diff old vanilla vs new vanilla before touching fork code
2. review enum and operation-surface changes first
3. run semantic-crate tests before broad runtime tests
4. explicitly check for new bypass paths around our adapters
5. reject ownership regressions that push doctrine back into shared seam files

Useful change buckets:

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

Any upstream change in those buckets requires custom-feature review.

## Mandatory enum and operation review

Always review these changes manually before doing any fork adaptation:

- new `ResponseItem` variants
- changed `ContentItem` shape
- new response call/output pair variants
- new `EventMsg` variants
- new `AgentStatus` variants
- new `SessionSource` or `SubAgentSource` variants
- new `ToolHandlerKind`
- new compact/config/protocol operation surfaces

These are the highest-risk upstream changes because they can compile while
silently changing semantic behavior.

## Test order

Run semantic-owner tests first:

```text
codex-context-maintenance-policy tests
codex-agent-observability tests
codex-tools tool_config/tool_registry_plan tests
```

Then run the high-signal adapter/runtime tests:

```text
core compact tests
core compact_remote tests
core compact_resume_fork tests
core context-maintenance tests
core agent_progress handler tests
core multi-agent handler tests
app-server refresh/prune/context tests
```

Snapshot changes around compaction must be reviewed manually. Do not
auto-accept them.

## Context-maintenance watchlist

### 1. Protocol item shape

Watch:

- `protocol/src/models.rs`
  - `ResponseItem`
  - `ContentItem`
  - `FunctionCallOutputPayload`
  - `content_items_to_text`
  - `function_call_output_content_items_to_text`
  - `remove_corresponding_response_item`

Why it matters:

- maintenance artifacts are represented as `ResponseItem`
- source selection filters over `ResponseItem`
- remote history shaping filters over `ResponseItem`
- thread-memory trimming depends on call/output pairing

Key question:

> Should the new or changed protocol item be retained, dropped, paired,
> summarized, or excluded from maintenance artifact source input?

### 2. Context manager history exposure

Watch:

- `core/src/context_manager/history.rs`
  - `record_items`
  - `for_prompt`
  - `raw_items`
  - history replacement behavior
- `core/src/context_manager/normalize.rs`
  - call-output normalization
  - orphan-output removal
  - image stripping
  - any response-item pairing logic
- `core/src/context_manager/updates.rs`

Why it matters:

- policy artifacts are generated from prompt/history items
- final history replacement must not leave orphaned or invalid protocol
  sequences
- source-selection assumptions depend on what `for_prompt` exposes

Key question:

> Did upstream change which history items are visible to compact or artifact
> generation, or how invalid call/output sequences are repaired?

### 3. Local compact path

Watch:

- `core/src/compact.rs`
  - `run_inline_auto_compact_task`
  - `run_compact_task`
  - `run_compact_task_inner`
  - `run_compact_task_inner_impl`
  - `should_use_remote_compact_task`
  - `InitialContextInjection`
  - `build_compacted_history`
  - `collect_user_messages`
  - `is_raw_conversation_message`
  - `is_real_user_message`
  - `is_user_or_summary_message`
  - `is_summary_message`

Why it matters:

- local compact is still the path most likely to satisfy disposition by
  construction rather than through a shared executor
- injection and source-history timing remain core adapter responsibilities
- summary-shaping changes can invalidate thread-memory source filters

Key question:

> Did upstream introduce a new compact timing, source-history shape, summary
> marker, or replacement-history assembly rule?

### 4. Remote compact path

Watch:

- `core/src/compact_remote.rs`
  - `run_inline_remote_auto_compact_task`
  - `run_remote_compact_task`
  - `run_remote_compact_task_inner_impl`
  - `process_compacted_history`
  - `should_keep_compacted_history_user_message`
  - `is_compaction_summary_message`
  - `trim_function_call_history_to_fit_context_window`
  - model-client compact call handling

Why it matters:

- remote compact output is shaped by policy but classified by core predicates
- upstream remote compact changes can alter insertion point, marker
  preservation, developer-message retention, or summary handling

Key question:

> Does the policy-owned remote history shape still match upstream remote compact
> semantics?

### 5. Runtime entrypoints and dispatch

Watch:

- `core/src/codex.rs`
  - compact, refresh, and prune dispatch
  - `run_pre_sampling_compact`
  - `maybe_run_previous_model_inline_compact`
  - `run_auto_compact`
  - `build_initial_context`
  - `build_prompt`
  - `replace_history`
  - `replace_compacted_history`
  - `clone_history`
  - `reference_context_item`
  - `build_settings_update_items`
  - `TurnContext` construction and fields

Why it matters:

- this is the hot runtime seam where timing and history replacement are
  triggered
- upstream can add new pre-sampling, post-tool, or prompt-building behavior
  that bypasses the policy adapter

Key question:

> Did upstream create a new context-maintenance-like path that does not call the
> policy adapter?

### 6. Event mapping and turn-item parsing

Watch:

- `core/src/event_mapping.rs`
  - `parse_turn_item`
  - user or developer contextual message detection
  - hook or contextual message classification

Why it matters:

- policy history shaping depends on core-supplied classifiers
- remote compact keep or drop law uses these classifiers

Key question:

> Did upstream change what counts as a real user message, contextual user
> message, hook message, or summary-like message?

### 7. Config and protocol surfaces

Watch:

- `config/src/config_toml.rs`
- `core/src/config/mod.rs`
- `core/config.schema.json`
- `protocol/src/protocol.rs`
  - `Op`
  - `Op::kind`
  - `ThreadMemoryMode`
- `app-server-protocol/src/protocol/common.rs`
- `app-server-protocol/src/protocol/v2.rs`
- `app-server/src/codex_message_processor.rs`

Why it matters:

- policy must remain decoupled from config/TOML shapes
- core adapters must keep translating config and operation surfaces into
  policy-owned types

Key questions:

> Did a config enum or default change require only adapter updates?
>
> Did upstream alter request or operation dispatch in a way that bypasses
> refresh or prune maintenance tasks?

### 8. Model client and task execution

Watch:

- `core/src/client.rs`
- model streaming APIs
- `compact_conversation_history`
- response-format/schema plumbing
- reasoning or model request configuration
- `core/src/tasks/compact.rs`
- `core/src/tasks/mod.rs`

Why it matters:

- continuation bridge and thread memory generation use model-client streaming
  and schema/result parsing
- remote compact depends on upstream model-client compact APIs

Key question:

> Did model streaming or response-format handling change such that artifact
> generation needs adapter updates?

## Agent-observability watchlist

### 1. EventMsg

Watch:

- `protocol/src/protocol.rs`
  - `EventMsg`
  - event structs used by the progress reducer

Why it matters:

- observability reducer law is a classification of `EventMsg`
- new event variants can compile while being silently ignored

Key question:

> Is the new or changed event material progress, heartbeat, blocker, terminal,
> or intentionally ignored?

### 2. AgentStatus and lifecycle derivation

Watch:

- `protocol/src/protocol.rs::AgentStatus`
- `core/src/agent/status.rs`
  - `agent_status_from_event`
  - `is_final`
- lifecycle-status tool schema in `tools/src/agent_progress_tool.rs`

Why it matters:

- snapshots output `AgentStatus`
- progress phase can be overridden by lifecycle status
- tool output schema must match protocol status serialization

Key question:

> Does every serialized lifecycle status have a valid tool schema and a
> deliberate progress interpretation?

### 3. Session and subagent source concepts

Watch:

- `protocol/src/protocol.rs`
  - `SessionSource`
  - `SubAgentSource`
  - `AgentPath`
  - `InterAgentCommunication`
  - `ThreadId`

Why it matters:

- `AgentToolSurfacePolicy` derives tool exposure from session source
- target resolution and thread-spawn containment depend on these shapes

Key question:

> Did upstream add a new session source or subagent source that needs explicit
> tool-surface policy?

### 4. Agent runtime control

Watch:

- `core/src/agent/control.rs`
  - `spawn_agent_internal`
  - `spawn_agent_with_metadata`
  - `resume_agent`
  - `seed_agent_progress`
  - `record_progress_event`
  - `subscribe_progress_seq`
  - `inspect_agent_progress`
  - `get_status`
  - `get_agent_metadata`
  - `open_thread_spawn_children`

Why it matters:

- this is the runtime adapter between live threads and the observability crate
- upstream changes to spawn, resume, or removal can create stale registry
  entries or missed seed events

Key question:

> Does every agent lifecycle path seed, update, inspect, and remove progress
> state correctly?

### 5. Event delivery path

Watch:

- `core/src/codex.rs`
  - `deliver_event_raw`
  - `send_event_raw`
  - agent status update call
  - progress recording hook
  - parent or child terminal notification paths

Why it matters:

- observability depends on seeing emitted events
- upstream may split event delivery, buffer events, or introduce a new path
  that bypasses the hook

Key question:

> Does every emitted event that should affect progress still pass through
> `record_progress_event` exactly once?

### 6. Thread manager lifetime

Watch:

- `core/src/thread_manager.rs`
  - `ThreadManagerState`
  - registry initialization
  - thread removal
  - resume or restore path

Why it matters:

- `ProgressRegistry` lives in thread-manager state
- upstream lifecycle changes can leak or drop progress state incorrectly

Key question:

> Does registry lifetime still match thread lifetime?

### 7. Progress tool handlers

Watch:

- `core/src/tools/handlers/agent_progress.rs`
- `core/src/tools/handlers/multi_agents_common.rs`
- `core/src/tools/handlers/multi_agents/*`
- target resolution helpers

Why it matters:

- wait loops remain runtime-side
- target resolution is deliberately not owned by the observability crate
- upstream may change target identity, agent paths, or timeout conventions

Key question:

> Are handler wait loops still only executing runtime waiting, not re-authoring
> wait-match law?

### 8. Tool planning and registry

Watch:

- `tools/src/tool_config.rs`
  - `ToolsConfig::new`
  - `AgentToolSurfacePolicy`
- `tools/src/tool_registry_plan.rs`
- `tools/src/tool_registry_plan_types.rs`
  - `ToolHandlerKind`
- `tools/src/agent_progress_tool.rs`
- `tools/src/tool_config_tests.rs`
- `tools/src/tool_registry_plan_tests.rs`

Why it matters:

- tool names and schemas must stay single-sourced from
  `codex-agent-observability`
- recursive spawn containment must stay explicit in `codex-tools`

Key question:

> Did upstream add or rename a tool-surface flag that should be represented in
> `AgentToolSurfacePolicy`?

### 9. Core tool routing and spec wiring

Watch:

- `core/src/tools/spec.rs`
- `core/src/tools/router.rs`
- `core/src/tools/spec_tests.rs`

Why it matters:

- core should only wire handler kinds and routing
- tool contract strings should continue to come from observability constants

Key question:

> Did core reintroduce hardcoded progress tool names or schema doctrine?

## New bypass-path review

For each upstream release, explicitly ask:

Context maintenance:

- Is there a new compact path that bypasses `runtime_plan_for_compact`?
- Is there a new refresh/prune-like operation that bypasses
  `runtime_plan_for_turn_boundary_maintenance`?
- Is there a new history replacement path that bypasses policy disposition?
- Is there a new prompt-building path that changes artifact source visibility?

Agent observability:

- Is there a new event delivery path that bypasses `record_progress_event`?
- Is there a new agent lifecycle path that skips seed or remove?
- Is there a new session/source type that bypasses `AgentToolSurfacePolicy`?
- Is there a new progress-visible event ignored by the reducer?

## Ownership regression checks

Reject or refactor ingests that reintroduce these patterns:

Context maintenance:

- `context-maintenance-policy` imports `codex-core` or `codex-config`
- core reimplements route law instead of consuming `MaintenancePolicyPlan`
- prompts, schemas, or tags for thread memory or continuation bridge move back
  into core
- generic `ResponseItem` utility logic moves back into policy

Agent observability:

- `agent-observability` imports `codex-core` or `codex-tools`
- core reimplements phase transitions or wait-match law
- tool names or phase strings are hardcoded again in core or tools
- thread-spawn containment becomes loose booleans again instead of
  `AgentToolSurfacePolicy`

## Practical grep gates

Run these on the old and new upstream vanilla baselines before adapting fork
code.

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

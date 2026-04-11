# Custom Governance Path Build Guide (Code-Grounded)

This document describes what is implemented in this fork today, based on the current code in `codex-rs`.

It is intentionally operational and code-grounded, not conceptual.

## Scope

The custom lane currently spans four concrete areas:

1. Continuation bridge artifacts injected before compaction.
2. ODEU thread-memory artifacts injected before compaction.
3. Strict-v1 governance packet/layering scaffolding.
4. E-witness live progress inspection/wait tools for sub-agents.
5. Fail-closed strict compaction behavior when required thread-memory
   generation fails.
6. Thread-spawn sub-agent tool containment so child lanes keep observability
   but do not recursively spawn more agents.

## 1) Continuation Bridge (Implemented)

### What it does

Before local or remote compaction, Codex can generate a structured continuation handoff as a tagged `developer` message:

- `<continuation_bridge schema="..."> ... </continuation_bridge>`

This handoff is inserted into `replacement_history` so the successor instance receives a compact, structured baton.

### Core implementation

- Generator and parser:
  - `codex-rs/core/src/continuation_bridge.rs`
- Bridge variants:
  - `codex-rs/core/src/continuation_bridge/baton.rs`
  - `codex-rs/core/src/continuation_bridge/rich_review.rs`
- Prompt/schema artifacts:
  - `codex-rs/core/templates/continuation_bridge/variants/baton/prompt.md`
  - `codex-rs/core/templates/continuation_bridge/variants/baton/schema.json`
  - `codex-rs/core/templates/continuation_bridge/variants/rich_review/prompt.md`
  - `codex-rs/core/templates/continuation_bridge/variants/rich_review/schema.json`

### Variants

- `continuation_bridge_baton_v1` (lean baton pass).
- `continuation_bridge_v2` (richer review-oriented handoff, with legacy `continuation_bridge_v1` accepted by parser).

### Model control for bridge generation

Bridge generation can use a dedicated model/reasoning effort, with fallback to resident model when unavailable/unsupported.

- Defaults in code:
  - model: `gpt-5-codex-mini`
  - reasoning effort: `high`
- Config fields:
  - `continuation_bridge_variant`
  - `continuation_bridge_model`
  - `continuation_bridge_reasoning_effort`
  - `continuation_bridge_prompt` and `continuation_bridge_prompt_file`

See:

- `codex-rs/core/src/config/mod.rs`
- `codex-rs/core/src/codex.rs` (`continuation_bridge_prompt`, `continuation_bridge_variant`)

### Compaction integration

Bridge generation is called in both compaction paths:

- Local compaction:
  - `codex-rs/core/src/compact.rs`
- Remote compaction:
  - `codex-rs/core/src/compact_remote.rs`

In both paths, bridge items are treated as authoritative and inserted before summary/compaction marker in replacement history.

## 2) ODEU Thread Memory (Implemented)

### What it does

When governance mode is enabled (`governance_path_variant != off`), compaction builds and injects a structured thread-memory artifact:

- `<thread_memory schema="odeu_thread_memory_v1"> ... </thread_memory>`

The updater treats the latest prior thread-memory artifact as canonical base and applies only delta from later source items.

### Core implementation

- `codex-rs/core/src/governance/thread_memory.rs`
- Prompt/schema artifacts:
  - `codex-rs/core/templates/thread_memory/prompt.md`
  - `codex-rs/core/templates/thread_memory/schema.json`

### Update mechanics currently implemented

- Uses latest tagged `<thread_memory ...>` message as previous canonical memory.
- Filters source items to avoid feeding prior thread-memory/continuation-bridge artifacts back into delta input.
- Caps source items to `MAX_SOURCE_ITEMS = 160`.
- Uses output schema enforcement for structured JSON-only output.

### Compaction integration

Thread-memory generation is invoked in:

- `codex-rs/core/src/compact.rs`
- `codex-rs/core/src/compact_remote.rs`

Gated by `governance_path_variant`:

- `off`: no thread-memory artifact.
- `strict_v1_shadow` / `strict_v1_enforce`: thread-memory artifact enabled.

### Fail-closed behavior and raw-window trimming

Strict thread-memory compaction now fails closed when required memory generation
returns empty output or errors before local/remote compaction completes.

Implemented in:

- `codex-rs/core/src/compact.rs`
- `codex-rs/core/src/compact_remote.rs`

Current behavior:

- local compaction emits an explicit error event before aborting when required
  thread-memory generation fails
- remote compaction aborts rather than silently dropping required memory
- strict compaction trims preserved raw conversation more aggressively once the
  authoritative `thread_memory` artifact is present

### Memory-mode contamination marker

If `memories.no_memories_if_mcp_or_web_search = true`, MCP tool calls and web-search calls mark thread memory mode as polluted in state DB:

- `codex-rs/core/src/mcp_tool_call.rs`
- `codex-rs/core/src/stream_events_utils.rs`
- Bridge re-export:
  - `codex-rs/core/src/state_db_bridge.rs`

## 3) Strict-v1 Governance Path (Implemented as Scaffolding + Prompt Layering)

### Config mode

- `governance_path_variant` enum:
  - `off`
  - `strict_v1_shadow`
  - `strict_v1_enforce`

Defined in:

- `codex-rs/core/src/config/mod.rs`

### Typed packet model

Implemented packet types and force typing:

- `NormativeForce`: `hard | default | advisory | factual`
- Layers:
  - constitution
  - role
  - task_charter
  - task_residual
  - runtime_fact_store
  - adapter_overlay

Defined in:

- `codex-rs/core/src/governance/packets.rs`

### Compiler + provenance witness

Implemented:

- packet compilation with duplicate-layer checks and constitution requirement in strict modes.
- projection witness with accepted/rejected/ambiguity records.

Defined in:

- `codex-rs/core/src/governance/compiler.rs`

### Diagnostics taxonomy + transition legality rules

Implemented:

- diagnostic severities, codes, violation kinds.
- legality checker for transitions:
  - `spawn`
  - `compact`
  - `resume`
  - `swap`
- mode behavior:
  - `off`: allow
  - `strict_v1_shadow`: allow with diagnostics
  - `strict_v1_enforce`: block on errors

Defined in:

- `codex-rs/core/src/governance/diagnostics.rs`
- `codex-rs/core/src/governance/transitions.rs`

Current status: these transition checks are implemented and tested, but not yet wired as a runtime gate in the main execution path.

### Prompt layering contract (active in runtime)

A `<governance_prompt_layers ...>` tagged payload is generated and injected into developer context for strict modes.

Defined in:

- `codex-rs/core/src/governance/prompt_layers.rs`
- contract text:
  - `codex-rs/core/templates/governance/prompt_layering.md`

Wired in:

- initial context building:
  - `codex-rs/core/src/codex.rs`
- settings update building:
  - `codex-rs/core/src/context_manager/updates.rs`

Recent fix included:

- Keep `<model_switch>` first when present; insert governance section after it.
- Build settings-update governance payload from full active role/task/runtime sections (not only diff fragments).

## 4) E-Witness Lane for Sub-Agents (Implemented)

### What it does

Adds live, normalized sub-agent progress observability without exposing raw hidden reasoning.

Two tools are exposed (when collaboration tools are enabled):

- `inspect_agent_progress`
- `wait_for_agent_progress`

### Core implementation

- progress reducer/registry:
  - `codex-rs/core/src/agent/progress.rs`
- control-plane API:
  - `codex-rs/core/src/agent/control.rs`
- tool handlers:
  - `codex-rs/core/src/tools/handlers/agent_progress.rs`
- tool specs:
  - `codex-rs/tools/src/agent_progress_tool.rs`
  - `codex-rs/core/src/tools/spec.rs`

### Thread-spawn containment rule

Thread-spawned sub-agents keep the E-witness/control-plane tools needed for
supervision, but do not receive `spawn_agent`.

Implemented in:

- `codex-rs/core/src/tools/spec.rs`
- tests:
  - `codex-rs/core/src/tools/spec_tests.rs`

Current effect:

- thread-spawned children still expose:
  - `inspect_agent_progress`
  - `wait_for_agent_progress`
  - wait/send/close tools appropriate to their v1/v2 collaboration mode
- thread-spawned children do not expose `spawn_agent`
- this is a containment measure for recursive delegation, not a full solution
  to semantic role contamination

### Snapshot fields implemented

`inspect_agent_progress` and `wait_for_agent_progress` report normalized fields including:

- `phase`
- `blocked_on`
- `active_work`
- `recent_updates`
- `latest_visible_message`
- `final_message`
- `error_message`
- `ever_entered_turn`
- `ever_reported_progress`
- `seq` (material progress sequence)
- `stalled`

### Wait semantics implemented

`wait_for_agent_progress` can wait on:

- `seq` advancing beyond `since_seq`
- phase match (`until_phases`)
- timeout (with bounded/clamped timeout range)

## 5) Known Limitations / Current Boundary

1. Strict-v1 transition legality is implemented but currently not yet enforced inline in runtime transitions.
2. Thread-memory and continuation-bridge are additive artifacts around compaction; server `/responses/compact` remains canonical transcript compactor.
3. Prompt-layering payload is an active guidance/diagnostic channel, not yet a hard execution planner.
4. Thread-spawn `spawn_agent` suppression reduces recursive delegation blast radius, but it does not by itself solve inherited-role contamination in forked contexts.

## 6) Minimal Config Example

```toml
# Enable strict governance lane
governance_path_variant = "strict_v1_shadow"

# Continuation bridge controls
continuation_bridge_variant = "baton" # or "rich_review"
continuation_bridge_model = "gpt-5-codex-mini"
continuation_bridge_reasoning_effort = "high"

# Optional custom bridge prompt
# continuation_bridge_prompt_file = "/absolute/path/to/prompt.txt"
```

# ODEU Memory Implementation Pseudocode

This document maps the ODEU memory protocol onto the current Codex fork in implementation terms.

It is intentionally not Rust code yet. The goal is to make the intended control flow, file touch
set, and failure boundaries explicit before patching the repo.

## Scope

This spec covers a local-compaction experiment where:

- `continuation_bridge` remains the short-horizon baton for immediate continuation
- `thread_memory` becomes the long-horizon semantic memory object
- `odeu_delta` blocks become the preferred semantic ingress format during normal `work` turns
- old transcript history is superseded by semantic memory after successful compaction

This spec does not try to redesign remote compaction. The intent is to force local compaction for
threads that opt into `thread_memory`.

## Design Constraints

- Touch as little existing compaction code as possible.
- Keep most new logic in a dedicated module tree rather than growing existing hot files.
- Fail open to the current local summary compaction flow if the structured-memory path breaks.
- Preserve the active user request during mid-turn compaction until we have evidence that
  `continuation_bridge` alone is sufficient.
- Ignore casual chat when the user explicitly marks the turn as `chill`.

## Minimal Touch Set

Existing files that likely need small edits:

- `/home/rose/work/codex/fork/codex-rs/core/src/config/mod.rs`
- `/home/rose/work/codex/fork/codex-rs/core/src/config/config_tests.rs`
- `/home/rose/work/codex/fork/codex-rs/core/src/compact.rs`
- `/home/rose/work/codex/fork/codex-rs/core/src/tasks/compact.rs`
- `/home/rose/work/codex/fork/codex-rs/core/src/codex.rs`
- `/home/rose/work/codex/fork/docs/config.md`

New files or modules:

- `/home/rose/work/codex/fork/codex-rs/core/src/thread_memory/mod.rs`
- `/home/rose/work/codex/fork/codex-rs/core/src/thread_memory/config.rs`
- `/home/rose/work/codex/fork/codex-rs/core/src/thread_memory/parser.rs`
- `/home/rose/work/codex/fork/codex-rs/core/src/thread_memory/merge.rs`
- `/home/rose/work/codex/fork/codex-rs/core/src/thread_memory/prompting.rs`
- `/home/rose/work/codex/fork/codex-rs/core/templates/thread_memory/variants/odeu_v0/prompt.md`
- `/home/rose/work/codex/fork/codex-rs/core/templates/thread_memory/variants/odeu_v0/schema.json`

Tests to add or extend:

- `/home/rose/work/codex/fork/codex-rs/core/src/compact_tests.rs`
- `/home/rose/work/codex/fork/codex-rs/core/tests/suite/compact.rs`

## Proposed Config Surface

The exact names can still change, but the implementation should expose three distinct concerns:

1. Compaction transport selection
2. Structured-memory variant selection
3. Prompt override for the structured-memory updater

Pseudo-shape:

```rust
enum CompactionBackend {
    Auto,
    Local,
    Remote,
}

enum ThreadMemoryVariant {
    Off,
    ODEUV0,
}

struct Config {
    compaction_backend: Option<CompactionBackend>,
    thread_memory_variant: Option<ThreadMemoryVariant>,
    thread_memory_prompt: Option<String>,
    thread_memory_prompt_file: Option<AbsolutePathBuf>,
}
```

Behavior:

- `thread_memory_variant = off`
  - current behavior
- `thread_memory_variant = odeu_v0`
  - structured-memory experiment enabled
  - compaction should behave as local even if provider would normally route remote
- `compaction_backend`
  - remains generally useful
  - but `thread_memory_variant != off` should override `auto` and force local for safety

## High-Level Runtime Behavior

### Normal turns

Pseudo-rule:

```rust
fn get_base_instructions() -> BaseInstructions {
    let mut text = session_configuration.base_instructions.clone();

    if config.thread_memory_variant == Some(ODEUV0) {
        text.push_str(THREAD_MEMORY_TURN_PROTOCOL_SNIPPET);
    }

    BaseInstructions { text }
}
```

The injected snippet should be short and operational:

- default turn mode is `work`
- if user explicitly tags a turn with `<turn_mode>chill</turn_mode>`, do not emit `odeu_delta`
- emit `odeu_delta` only when semantic state changes
- use family id plus revision fields
- prefer delta updates over prose recap

Important restriction:

- only assistant final/output messages are eligible semantic ingress
- do not treat commentary/progress/tool status text as `odeu_delta` candidates

### Compaction routing

Pseudo-rule:

```rust
fn should_use_remote_compact_task(provider: &ModelProviderInfo, config: &Config) -> bool {
    if config.thread_memory_variant != Some(Off) {
        return false;
    }

    match config.compaction_backend.unwrap_or(Auto) {
        Auto => provider.is_openai(),
        Local => false,
        Remote => true,
    }
}
```

This keeps the experiment isolated:

- normal sessions can still use current remote compaction behavior
- memory sessions always take the local path

## New Local Structured Compaction Flow

The cleanest implementation is not to overload the current summary path inline. Add a dedicated
branch inside local compaction that reuses existing pre-compaction steps:

- emit `ContextCompactionItem`
- generate `continuation_bridge`
- collect current history snapshot

Then diverge only for the replacement-history build.

Pseudo-code:

```rust
async fn run_compact_task_inner(...) -> CodexResult<()> {
    emit_compaction_started();

    let continuation_bridge_item = generate_continuation_bridge_item(...).await.ok();

    history.record_items(&[initial_input_for_turn.into()], truncation_policy);

    let summary_suffix = run_current_local_compaction_model_turn(...).await?;

    if thread_memory_enabled(sess) {
        if let Ok(compacted_item) = build_thread_memory_compacted_item(
            sess,
            turn_context,
            initial_context_injection,
            continuation_bridge_item,
            summary_suffix,
        )
        .await
        {
            replace_history(compacted_item).await;
            finish_compaction().await;
            return Ok(());
        }

        warn!("thread_memory compaction failed; falling back to summary compaction");
    }

    let compacted_item = build_existing_summary_compacted_item(
        sess,
        turn_context,
        initial_context_injection,
        continuation_bridge_item,
        summary_suffix,
    )
    .await?;

    replace_history(compacted_item).await;
    finish_compaction().await;
    Ok(())
}
```

## Structured-Memory Builder

Add one entry point that owns the new logic:

```rust
async fn build_thread_memory_compacted_item(
    sess: &Session,
    turn_context: &TurnContext,
    initial_context_injection: InitialContextInjection,
    continuation_bridge_item: Option<ResponseItem>,
    summary_suffix: String,
) -> CodexResult<CompactedItem>
```

Responsibilities:

1. Read the last stored `thread_memory` object from current history, if any.
2. Identify the transcript slice after that memory checkpoint.
3. Extract turn modes and eligible `odeu_delta` blocks from that slice.
4. Compute structured updates.
5. Call a normal `/responses` run with a JSON schema to emit the next canonical `thread_memory`.
6. Build replacement history without old transcript history.

## Memory Artifact Discovery

The persisted memory object should be stored like the bridge: as a tagged developer item inside
replacement history.

Suggested format:

```text
<thread_memory schema="odeu_thread_memory@1" epoch="4">
{ ...json... }
</thread_memory>
```

Pseudo-code:

```rust
fn find_latest_thread_memory(items: &[ResponseItem]) -> Option<ThreadMemoryEnvelope> {
    items.iter()
        .enumerate()
        .filter_map(parse_thread_memory_item)
        .last()
}
```

Returned data:

- history index where the last memory artifact was found
- parsed JSON payload
- epoch number

## Delta Slice Extraction

Once the latest memory artifact is known:

```rust
fn items_since_last_thread_memory(
    items: &[ResponseItem],
    latest_memory: Option<&ThreadMemoryEnvelope>,
) -> &[ResponseItem] {
    match latest_memory {
        Some(memory) => &items[memory.history_index + 1..],
        None => items,
    }
}
```

Filtering rules:

- ignore prior `thread_memory` items
- ignore prior `continuation_bridge` items
- ignore ghost snapshots for semantic updates
- ignore system/developer replacement-history scaffolding
- keep user messages and assistant final messages

## Turn Mode Extraction

The updater needs an explicit view of which user turns are `work` or `chill`.

Pseudo-code:

```rust
struct TurnModeRecord {
    user_turn_id: String,
    mode: TurnMode, // Work | Chill
    raw_text: String,
}

fn extract_turn_modes(items: &[ResponseItem]) -> Vec<TurnModeRecord> {
    for user_message in parsed_user_messages(items) {
        let mode = parse_turn_mode_tag(user_message.text).unwrap_or(TurnMode::Work);
        records.push(TurnModeRecord { ... });
    }
    records
}
```

Compaction semantics:

- `chill` user turns are ignored for memory writes
- assistant turns answering a `chill` user turn must not contribute `odeu_delta`

## Delta Extraction

Only assistant final messages from `work` turns are eligible.

Pseudo-code:

```rust
struct ODEUDeltaEnvelope {
    family_id: String,
    revision: u32,
    status: DeltaStatus,
    supersedes: Option<String>,
    subject_id: Option<String>,
    body: ParsedODEUDelta,
    assistant_history_index: usize,
    parent_user_turn_id: Option<String>,
}

fn extract_odeu_deltas(
    items: &[ResponseItem],
    turn_modes: &[TurnModeRecord],
) -> Vec<ODEUDeltaEnvelope> {
    for assistant_message in parsed_assistant_output_messages(items) {
        if parent_turn_mode(assistant_message, turn_modes) == Chill {
            continue;
        }

        deltas.extend(parse_odeu_delta_blocks(assistant_message.text));
    }
}
```

Important v0 rule:

- if an assistant work turn contains no `odeu_delta`, the updater does not invent new durable
  semantic state from assistant prose alone
- the only exception is direct user corrections or direct user constraints that should be preserved
  as unresolved pressure in memory

## User-Correction Detection

Compaction should resolve contradictions conservatively.

Pseudo-code:

```rust
struct UserCorrection {
    affected_family_id: Option<String>,
    description: String,
    user_history_index: usize,
}

fn detect_user_corrections(items: &[ResponseItem]) -> Vec<UserCorrection> {
    // v0 heuristic:
    // - explicit references to delta ids
    // - clear contradiction markers ("no", "that's wrong", "not that", "instead")
    // - direct restatement of constraint/objective after a delta
}
```

Conservative rule:

- if a family is contradicted by the user and no later assistant revision exists, it must not
  remain accepted in `thread_memory`
- instead it becomes an uncertainty, pending correction, or open loop

## Revision Resolution

Pseudo-code:

```rust
fn resolve_delta_families(
    deltas: Vec<ODEUDeltaEnvelope>,
    user_corrections: &[UserCorrection],
) -> Vec<ResolvedDelta> {
    let mut families = group_by_family_id(deltas);

    for family in families.values_mut() {
        sort_by_revision(family);
        discard_duplicate_or_invalid_revisions(family);
    }

    let mut resolved = Vec::new();
    for family in families {
        let latest = family.latest_non_withdrawn_revision();

        if contradicted_without_followup(&latest, user_corrections) {
            resolved.push(latest.as_pending_correction());
            continue;
        }

        resolved.push(latest.into_resolved());
    }

    resolved
}
```

## Memory-Updater Prompt

The updater should not summarize the whole thread. It should receive:

- previous canonical `thread_memory`
- a compact list of eligible `odeu_delta` envelopes
- extracted user corrections and direct user constraints
- explicit merge instructions

Pseudo-code:

```rust
async fn generate_updated_thread_memory(
    sess: &Session,
    turn_context: &TurnContext,
    previous_memory: Option<&ThreadMemoryEnvelope>,
    resolved_deltas: &[ResolvedDelta],
    direct_user_updates: &[DirectUserUpdate],
) -> CodexResult<ThreadMemoryEnvelope> {
    let prompt = Prompt {
        input: build_thread_memory_prompt_input(previous_memory, resolved_deltas, direct_user_updates),
        base_instructions: thread_memory_base_instructions(sess).await,
        output_schema: Some(load_thread_memory_schema()),
        personality: turn_context.personality,
        ..Default::default()
    };

    let response = run_single_structured_response(prompt).await?;
    parse_thread_memory_response(response)
}
```

Prompt rules:

- update structure, do not retell history
- only touch affected subject/global sections
- preserve stable identifiers where possible
- emit a full canonical memory snapshot, not a patch document

## Replacement History Builder

The output history for successful memory compaction should be much leaner than current local
summary compaction.

Pseudo-code:

```rust
fn build_thread_memory_replacement_history(
    initial_context_injection: InitialContextInjection,
    initial_context: Vec<ResponseItem>,
    current_turn_user_message: Option<ResponseItem>,
    thread_memory_item: ResponseItem,
    continuation_bridge_item: Option<ResponseItem>,
    ghost_snapshots: Vec<ResponseItem>,
) -> Vec<ResponseItem> {
    let mut history = Vec::new();

    if initial_context_injection == BeforeLastUserMessage {
        history.extend(initial_context);
    }

    if let Some(user_message) = current_turn_user_message {
        history.push(user_message);
    }

    history.push(build_thread_memory_usage_note());
    history.push(thread_memory_item);

    if let Some(bridge) = continuation_bridge_item {
        history.push(bridge);
    }

    history.extend(ghost_snapshots);
    history
}
```

Important conservative choice for v0:

- during mid-turn compaction, keep the active user message in replacement history
- during manual or between-turn compaction, do not keep older raw transcript

Reason:

- this avoids forcing `continuation_bridge` to carry the full active-request burden on day one
- it still removes historical transcript bulk

## CompactedItem Message

`CompactedItem.message` should remain a small compatibility string.

Pseudo-shape:

```text
Structured thread memory checkpoint saved.
Use thread_memory for durable semantic state and continuation_bridge for immediate continuation.
```

The heavy content should live in tagged developer items inside `replacement_history`.

## Failure Policy

The structured-memory path should fail open.

Pseudo-code:

```rust
match build_thread_memory_compacted_item(...).await {
    Ok(compacted_item) => use_it(),
    Err(err) => {
        warn!("structured memory compaction failed: {err}");
        fallback_to_existing_summary_compaction();
    }
}
```

Failure cases that should trigger fallback:

- latest `thread_memory` artifact is unparsable
- updater prompt construction fails
- structured `/responses` call fails
- returned JSON fails schema validation
- replacement history assembly detects a missing active user message in a mid-turn case

## Test Matrix

### Config tests

- `thread_memory_variant = "odeu_v0"` parses correctly
- inline prompt overrides file prompt
- memory variant forces local compaction routing even for OpenAI providers

### Parser tests

- `turn_mode` tag parsing
- `odeu_delta` block parsing
- multiple deltas in one assistant message
- family revision chain parsing

### Merge tests

- latest revision wins within one family
- `withdrawn` removes prior family from active memory
- user correction without follow-up revision downgrades accepted state
- untouched subjects remain byte-for-byte equal in canonical memory output where possible

### Local compaction tests

- first memory compaction with no prior memory artifact
- second memory compaction updates previous memory rather than rebuilding from scratch
- `chill` turn produces no memory update
- mid-turn compaction preserves active user message
- fallback path still emits current summary compaction when memory update fails

## Deliberate v0 Limits

- No remote-compaction preservation of `thread_memory`
- No sidecar mid-turn durable checkpoints outside compaction
- No attempt to infer durable assistant semantic state from untagged prose
- No sub-agent-specific memory protocol yet
- No separate `thread_memory_model` or `thread_memory_reasoning_effort` knob in v0 unless
  latency or quality makes it necessary

## Open Questions To Revisit Before Coding

1. Should `settled` only be emitted by the assistant after explicit user confirmation, or may the
   updater promote a family to `settled` when later accepted work clearly depends on it?
2. Should the tiny usage note live in history at all, or should static base instructions be enough?
3. Should direct user constraints/objectives create memory updates even when the assistant forgot to
   emit a new `odeu_delta`?
4. Is preserving the current-turn user message enough for mid-turn safety, or should we also keep a
   tiny active-turn envelope?
5. Do we want a separate `thread_memory_variant` for a stricter future mode that removes even the
   active user message on mid-turn compaction?

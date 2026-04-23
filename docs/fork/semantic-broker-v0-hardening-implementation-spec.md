# Semantic Broker v0 Hardening Implementation Spec (Post-0.123)

This spec defines the first hardening slice after the `0.123` ingest for the
new semantic-broker module and its adjacent observability seam.

Primary references:

- [semantic-broker-implementation-spec.md](/home/rose/work/codex/fork/docs/fork/semantic-broker-implementation-spec.md)
- [agent-observability-122-followup-implementation-spec.md](/home/rose/work/codex/fork/docs/fork/agent-observability-122-followup-implementation-spec.md)
- [release-alignment-log.md](/home/rose/work/codex/fork/docs/fork/release-alignment-log.md)

## Judgment

The architecture is correct and should stay as-is:

- `codex-semantic-broker` owns registry/candidate/adjudication/packet law.
- `core/src/semantic_broker_runtime.rs` stays a thin runtime adapter.
- `Feature::SemanticBroker` remains default-off.

There is one functional v0 bug to fix before calling broker ingress stable:

- current turn text is inferred from prompt tail scanning and can be lost when
  contextual user fragments (especially skill fragments) are appended after the
  actual user prompt in the same turn.

Secondary hardening in this family:

- make broker packet rendering robust against tag-breaking payload content.
- make observability reducer no-op coverage explicit for key wildcarded events.

## Scope

In scope:

1. Fix semantic broker current-turn text ingress (P0).
2. Harden active packet rendering envelope (P1).
3. Add explicit observability no-op classification for selected wildcarded
   variants (P1).
4. Reduce broker packet contract duplication in core tests (P2).

Out of scope:

- enabling `semantic_broker` by default.
- introducing durable broker artifacts in conversation history.
- protocol-level packet variants.
- broad governance-layer refactor in `session/mod.rs`.
- changing thread-memory governance semantics in this slice.

## Required Invariants

1. Broker remains prompt-only.
2. Broker remains semantic owner of packet tag/schema/render contract.
3. Core remains adapter-only; no second broker doctrine in `core`.
4. Observability reducer remains semantic owner of progress classification; no
   re-ownership in `core`.
5. Existing context-maintenance crate boundaries remain unchanged.

## Work Packages

### H1 (P0): Explicit current-turn text ingress for broker

Problem:

- `append_semantic_broker_prompt_overlay(...)` currently builds `BrokerInput`
  with `current_turn_text(&prompt_input)` from reverse prompt scanning.
- In the same turn, `run_turn(...)` records the user prompt and then appends
  contextual user fragments (`skill_items`, `plugin_items`) to history.
- Reverse scan hits the latest contextual user message first and can return
  `None`, losing the real same-turn user prompt.

Target design:

- `run_turn` / `run_sampling_request` passes explicit current-turn text into
  semantic broker runtime.
- Runtime adapter prefers this explicit text and does not rely on prompt-tail
  inference for the main turn path.
- Prompt scanning may remain only as fallback for non-standard call paths.

Required file changes:

- [turn.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/turn.rs)
- [semantic_broker_runtime.rs](/home/rose/work/codex/fork/codex-rs/core/src/semantic_broker_runtime.rs)
- [turn_tests.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/turn_tests.rs)

Required tests:

1. Keep the guard that broker does not fall back past a latest image-only user
   message.
2. Replace/adjust the contextual-user test doctrine so same-turn contextual
   fragments after a real user prompt do not erase current-turn text.
3. Keep the prompt-only overlay invariants (exactly one overlay in prompt input,
   no durable history mutation).

Acceptance criteria:

- A turn with user text plus trailing skill/contextual fragment produces broker
  resolution based on that same-turn user text (not unresolved by default).

### H2 (P1): Packet envelope hardening in semantic broker render path

Problem:

- broker packet JSON is embedded directly between XML-like tags:
  `<active_context_packet ...>{json}</active_context_packet>`.
- payload content containing tag-breaking substrings can corrupt the envelope.

Target design:

- escape `<`, `>`, `&` in payload JSON before embedding (for example via unicode
  escapes) while preserving valid JSON round-trip semantics.

Required file changes:

- [render.rs](/home/rose/work/codex/fork/codex-rs/semantic-broker/src/render.rs)
- broker render tests in the same module.

Required tests:

1. A malicious tool binding containing `</active_context_packet>` does not break
   packet envelope recognition.
2. Rendered payload still deserializes to expected packet contract.

Acceptance criteria:

- `is_active_context_packet_text(...)` remains true for escaped payloads.
- Packet contract parsing tests continue to pass.

### H3 (P1): Explicit no-op classification for wildcarded progress events

Problem:

- `codex-agent-observability` reducer still has a broad `_ => {}` arm.
- Key progress-adjacent events are currently implicit via wildcard.

Target design:

- explicitly classify selected variants as named no-ops (no seq bump, no phase
  mutation) in reducer match arms.
- keep current behavior unless explicitly intended to change.

Initial explicit no-op set for this slice:

- `AgentReasoningRawContent`
- `AgentReasoningSectionBreak`
- `ViewImageToolCall`
- `HookStarted`
- `HookCompleted`
- `CollabAgentSpawnBegin` / `CollabAgentSpawnEnd`
- `CollabAgentInteractionBegin` / `CollabAgentInteractionEnd`
- `CollabWaitingBegin` / `CollabWaitingEnd`
- `CollabCloseBegin` / `CollabCloseEnd`
- `CollabResumeBegin` / `CollabResumeEnd`

Required file changes:

- [progress.rs](/home/rose/work/codex/fork/codex-rs/agent-observability/src/progress.rs)

Required tests:

1. Add at least one representative no-op test proving seq/phase unchanged.
2. Keep existing explicit no-op tests (`PlanDelta`, `ReasoningRawContentDelta`)
   passing.

Acceptance criteria:

- reducer behavior is unchanged for these variants but no longer hidden behind
  wildcard.

### H4 (P2): Remove core packet-contract literal duplication

Problem:

- core broker tests hardcode packet tag/schema literals.

Target design:

- core tests assert behavior through broker-owned helpers or central constants
  rather than re-declaring packet envelope literals.

Required file changes:

- [turn_tests.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/turn_tests.rs)

Acceptance criteria:

- no runtime behavior change.
- less duplicate packet-contract doctrine outside broker crate.

## PR Sequence

Recommended sequencing:

1. `SB-H1` (P0): explicit current-turn ingress + test doctrine adjustment.
2. `SB-H2` (P1) + `SB-H4` (P2): packet escaping + test contract cleanup.
3. `AO-H1` (P1): explicit observability no-op classification.

Rationale:

- ship functional broker ingress fix first.
- keep packet/render hardening and observability hardening reviewable in small,
  independent PRs.

## Validation Plan

Run after each PR as relevant:

```bash
cd codex-rs
just fmt
cargo test -p codex-semantic-broker
cargo test -p codex-agent-observability
cargo test -p codex-core semantic_broker_ --lib
cargo test -p codex-core tools::handlers::agent_progress::tests --lib
```

If `codex-core` targeted filters miss touched tests, run explicit test names in
`core/src/session/turn_tests.rs` and `agent-observability/src/progress.rs`.

## Deferred Follow-ups (Not In This Slice)

1. Decide semantic meaning of `ThreadMemoryGovernance::Disabled`
   (naming or behavior alignment).
2. Reduce governance-layer custom contact in `session/mod.rs` using the 0.123
   context fragment substrate, without moving broker law into `core`.

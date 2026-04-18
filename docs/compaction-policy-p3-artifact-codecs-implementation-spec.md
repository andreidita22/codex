# Compaction Policy P3 Artifact Codecs Implementation Spec

## Purpose

This spec defines `p3` of the compaction-policy extraction:

- move artifact-family logic into `codex-context-maintenance-policy`
- make the new crate own tagged artifact codecs and artifact-specific
  prompt/spec assembly
- keep runtime behavior unchanged while artifact ownership moves

This step follows:

- [docs/compaction-policy-crate-extraction-plan.md](compaction-policy-crate-extraction-plan.md)
- [docs/compaction-policy-route-matrix.md](compaction-policy-route-matrix.md)
- [docs/compaction-policy-p0-route-matrix-implementation-spec.md](compaction-policy-p0-route-matrix-implementation-spec.md)
- [docs/compaction-policy-p1-scaffold-implementation-spec.md](compaction-policy-p1-scaffold-implementation-spec.md)
- [docs/compaction-policy-p2-history-shaping-implementation-spec.md](compaction-policy-p2-history-shaping-implementation-spec.md)

## Interpretation

`p3` is the artifact-layer extraction.

Unlike `p2`, which moved deterministic history shaping, this phase moves the
remaining fork-owned artifact semantics that are still smeared across
`codex-core`:

- prompt/spec builders
- tagged payload parsing
- schema normalization
- `ResponseItem` codecs
- artifact inventory helpers

This is still not the timing-law activation step. `p3` should preserve current
runtime behavior while changing ownership of artifact families.

## Recommended Internal Commit Order

`p3` is coherent as one phase, but it is materially larger than `p0`-`p2`.
Keep it reviewable by implementing it in this order even if it ultimately lands
as one PR:

1. shared tagged-artifact helpers and prune-manifest codecs
2. continuation-bridge artifact family
3. thread-memory artifact family
4. `codex-core` call-site rewiring and test migration

This keeps rollback cleaner and makes review easier if one artifact family
needs adjustment without backing out the whole phase.

## Scope

`p3` should do four things:

1. move continuation-bridge artifact logic into
   `codex-context-maintenance-policy`
2. move thread-memory artifact logic into
   `codex-context-maintenance-policy`
3. move shared tagged-artifact codecs and prune-manifest codecs into the new
   crate
4. rewire `codex-core` to use those new pure helpers while keeping model
   execution local to `codex-core`

`p3` should not:

- activate the target `TurnBoundary` vs `IntraTurn` law
- change the current `/refresh` overlap behavior
- change TUI, config, or app-server surfaces
- move `Session`, `TurnContext`, telemetry, or streaming logic out of
  `codex-core`
- redesign compaction routing or switch live runtime behavior to the `p1` route
  law

## Why P3 Comes After P2

`p2` extracted the deterministic history-shaping seam, but it intentionally
left artifact families split:

- `codex-context-maintenance-policy` owns placement/retention
- `codex-core` still owns what the artifacts mean

That split is unstable if left in place for later phases. The shaping layer
still depends on core-owned knowledge for:

- which tagged artifacts exist
- how to encode/decode them
- how to normalize model output into canonical artifact payloads
- how to find the latest prior artifact in history

`p3` closes that gap without yet changing the timing law.

## Current Code Surface To Move

### From `continuation_bridge.rs`

In [continuation_bridge.rs](/home/rose/work/codex/fork/codex-rs/core/src/continuation_bridge.rs):

- prompt selection and schema selection
- output schema artifacts
- bridge payload parsing
- bridge payload normalization
- `ResponseItem` encoding
- variant-specific prompt/schema assets now living under `baton.rs` and
  `rich_review.rs`

`generate_continuation_bridge_item(...)` should remain in `codex-core`, but the
artifact-family logic it calls should become policy-owned.

The canonical prompt/schema assets for this artifact family should move with
that logic. Do not leave duplicate authoritative copies behind in
`codex-core`.

### From `governance/thread_memory.rs`

In [thread_memory.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/thread_memory.rs):

- output schema artifact
- previous-memory extraction
- delta-source filtering
- source-item trimming
- update-message construction
- thread-memory response parsing
- schema normalization
- `ResponseItem` encoding
- tagged payload extraction

`generate_thread_memory_item(...)` should remain a core-owned execution wrapper,
but the artifact logic it calls should move.

The canonical prompt/schema assets for thread memory should also move with the
artifact logic. `codex-core` should not retain a second authoritative copy of
those templates after `p3`.

### From `context_maintenance.rs`

In [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs):

- tagged artifact classification
- tagged-block detection
- prune-manifest `ResponseItem` codec
- artifact supersession filtering
- explicit artifact-kind removal

These helpers were deferred in `p2` only because artifact classification still
lived in `codex-core`. Once tagging and codecs move, these should become
policy-owned too.

### Shared helper pressure from `compact.rs`

The artifact helpers currently depend on
[compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs) for
message-text extraction and summary-prefix assumptions.

`p3` should not leave the new crate depending on `codex-core` helpers. If an
artifact codec needs a small protocol-text utility, add it locally in
`codex-context-maintenance-policy`.

Treat that as a hard rule for this phase:

- add a small local protocol-text utility in the new crate
- cover it with tests that mirror the current behavior
- do not leave artifact parsing dependent on `codex-core::compact`

## Explicit Non-Scope For P3

These surfaces should stay in `codex-core` for now:

- `generate_continuation_bridge_item(...)` orchestration
- `generate_thread_memory_item(...)` orchestration
- request-context resolution from `Session` and `TurnContext`
- model session creation and streaming
- `subagent_context.rs` runtime-derived snapshot construction
- generic summary-message classification not tied to tagged artifact families

This means:

- `retain_latest_summary_message(...)` may stay in `codex-core` for `p3`
- `send_turn_started_event(...)` remains in `codex-core`
- analytics and telemetry remain in `codex-core`

`p3` is about artifact ownership, not runtime ownership.

## Proposed Policy-Crate Module Expansion

Extend the new crate with three modules:

- `artifact_codecs.rs`
  - tagged-block detection
  - artifact-kind classification
  - prune-manifest codec
  - latest-artifact extraction helpers
  - artifact supersession helpers
- `continuation_bridge/`
  - bridge prompt/spec assets
  - bridge payload types and parsers
  - bridge `ResponseItem` codec
- `thread_memory.rs`
  - thread-memory prompt/spec assets
  - thread-memory payload parser/normalizer
  - thread-memory `ResponseItem` codec
  - prior-memory/delta-source extraction helpers

Suggested updated crate layout:

- `src/contracts.rs`
- `src/route_matrix.rs`
- `src/history_shape.rs`
- `src/retention.rs`
- `src/artifact_codecs.rs`
- `src/thread_memory.rs`
- `src/continuation_bridge/mod.rs`
- `src/continuation_bridge/baton.rs`
- `src/continuation_bridge/rich_review.rs`
- `src/tests.rs`

Keep `p3` focused. Do not add `assemble.rs` yet, and do not activate route-law
driven artifact selection in this phase.

The new crate should own the canonical artifact-family assets after this phase.
That means prompt markdown, JSON schemas, and variant-local support files move
with the artifact modules rather than being copied and left in both trees.

## API Shape For P3

`p3` is the first step where the new crate should own artifact semantics
directly. But it should still avoid prematurely introducing a final large
assembly API.

Use helper-sized pure interfaces.

### 1. Continuation bridge artifact API

The policy crate should expose pure bridge helpers, for example:

```rust
pub enum BridgeVariant {
    Baton,
    RichReview,
}

pub struct ContinuationBridgeModelInput {
    pub source_items: Vec<ResponseItem>,
    pub supplemental_items: Vec<ResponseItem>,
    pub user_prompt_text: String,
}

pub fn build_continuation_bridge_prompt_input(
    input: ContinuationBridgeModelInput,
) -> Vec<ResponseItem>;

pub fn continuation_bridge_output_schema(variant: BridgeVariant) -> Value;

pub fn parse_continuation_bridge_payload(
    result: &str,
    fallback_variant: BridgeVariant,
) -> Result<ContinuationBridgePayload>;

pub fn continuation_bridge_response_item(
    payload: ContinuationBridgePayload,
) -> Result<ResponseItem>;
```

The important part is not exact naming. The important part is that prompt
assembly, parsing, normalization, and encoding become policy-owned and pure.

`BridgeVariant` and any other artifact-family enums introduced in this phase
should be policy-owned enums. Translate from config/core enums at the wrapper
boundary rather than importing upstream runtime enums deep into the new crate.

Runtime-derived enrichments such as subagent context remain core-owned in `p3`.
`codex-core` should build those items and pass them in as explicit supplemental
items, rather than forcing the policy crate to reconstruct session state.

### 2. Thread memory artifact API

The policy crate should expose pure thread-memory helpers, for example:

```rust
pub struct ThreadMemorySourceSelection {
    pub previous_memory: Option<serde_json::Value>,
    pub source_items: Vec<ResponseItem>,
}

pub fn split_previous_memory_and_source_items(
    input: &[ResponseItem],
) -> ThreadMemorySourceSelection;

pub fn build_thread_memory_update_message(
    previous_memory: Option<&serde_json::Value>,
    source_items_count: usize,
    trimmed_source_count: usize,
) -> String;

pub fn parse_thread_memory_payload(result: &str) -> Result<serde_json::Value>;

pub fn thread_memory_response_item(memory: serde_json::Value) -> Result<ResponseItem>;
```

`p3` does not need to invent a fully typed Rust struct for thread-memory
content. A normalized `serde_json::Value` wrapper is enough in this phase,
because the current schema is already externalized and model-shaped.

### 3. Shared artifact codec API

The policy crate should expose shared artifact-family helpers, for example:

```rust
pub fn tagged_artifact_kind(item: &ResponseItem) -> Option<ArtifactKind>;

pub fn remove_artifact_kind(
    items: Vec<ResponseItem>,
    kind: ArtifactKind,
) -> (Vec<ResponseItem>, usize);

pub fn prune_superseded_artifacts(
    items: Vec<ResponseItem>,
) -> (Vec<ResponseItem>, usize);

pub fn build_prune_manifest_item(
    original_len: usize,
    final_len: usize,
    removed_count: usize,
) -> Result<ResponseItem>;
```

If `p3` needs a small inventory type for current call sites, it should be
artifact-local and minimal, for example:

```rust
pub struct PriorArtifactInventory {
    pub latest_thread_memory: Option<ResponseItem>,
    pub latest_continuation_bridge: Option<ResponseItem>,
    pub latest_prune_manifest: Option<ResponseItem>,
}
```

This is still current-behavior scaffolding, not the final `p4`/`p5` assembly
surface.

If such an inventory helper is introduced, document it in code as temporary
artifact-local scaffolding. It must not silently become the final policy seam.

## Parse Diagnostics And Error Surface

`p3` moves parsing and normalization into the policy crate, but those helpers
already embody operational policy:

- how malformed JSON is handled
- what happens when trailing text exists after a JSON payload
- how unknown schema names are treated
- which normalization defaults are applied when fields are missing or empty

This behavior should become explicit in the new crate.

Recommended rule for `p3`:

- keep warning/preview generation logic close to the parser in the policy crate
- return structured parse failures to `codex-core` as `Result<...>`
- keep runtime event emission and high-level warning surfaces in `codex-core`

In other words:

- parse semantics and low-level diagnostics are policy-owned
- runtime surfacing remains core-owned

Do not leave malformed-output handling as an accidental side effect of copied
parser code.

## Ownership Rules For P3

The ownership split after `p3` should be:

### Policy crate owns

- artifact-family prompt/spec assets
- artifact payload parsers and normalizers
- tagged payload detection/extraction
- `ResponseItem` codecs for thread memory, continuation bridge, and prune
  manifest
- artifact-kind classification and supersession helpers

### `codex-core` owns

- when artifact generation runs
- model selection and reasoning selection
- request-context resolution
- model streaming and output collection
- warnings/analytics/event emission
- live orchestration for `/compact`, `/refresh`, `/prune`

This should be explicit in code structure, not only in doc prose.

## Prune Ordering Constraint

`p3` moves prune-manifest encoding, tagged artifact classification, artifact
supersession filtering, and explicit artifact-kind removal into the policy
crate, while intentionally leaving generic summary handling such as
`retain_latest_summary_message(...)` in `codex-core`.

That boundary is acceptable only if the current prune ordering remains stable.

`p3` must preserve the current interaction between:

1. artifact supersession filtering
2. explicit artifact-kind removal
3. latest-summary retention
4. prune-manifest insertion

Do not validate these only as isolated helpers. Add at least one combined
current-behavior fixture for the prune flow so ordering regressions are caught
at the boundary between policy-owned artifact logic and core-owned summary
logic.

## Behavioral Constraints

`p3` must preserve current behavior.

That means:

- `p0` current-state tests stay authoritative for current behavior
- `p1` route-matrix tests remain target-law tests only
- `p2` history-shaping behavior remains unchanged
- `/refresh` still regenerates both `thread_memory` and
  `continuation_bridge` in the live runtime
- `generate_thread_memory_item(...)` and
  `generate_continuation_bridge_item(...)` remain the live core entry points

In `p3`, new crate tests are still **current-behavior-preservation tests** for
artifact semantics, not timing-law activation tests.

## Test Plan

### New crate tests

Add direct tests in the new crate for:

- bridge payload parsing for both variants
- bridge schema normalization
- bridge `ResponseItem` encoding
- thread-memory payload parsing and schema normalization
- previous-thread-memory extraction and delta-source selection
- prune-manifest encoding
- tagged artifact classification
- artifact supersession filtering keeps only the latest artifact per kind
- local protocol-text extraction helpers used by artifact codecs

Also add at least one combined prune-order fixture that proves the extracted
artifact helpers still interact correctly with core-owned latest-summary
retention.

Where current tests already exist in `codex-core`, migrate or mirror them into
the new crate instead of rewriting behavior.

### `codex-core` guardrails

Keep these green:

- `cargo test -p codex-context-maintenance-policy`
- `cargo test -p codex-core compaction_policy_matrix_tests --lib`
- `cargo test -p codex-core compact::tests --lib`
- `cargo test -p codex-core context_maintenance::tests --lib`

If bridge or thread-memory unit tests still temporarily live under
`codex-core`, keep them green while moving them incrementally toward the new
crate.

## Proposed File Changes

### Docs

Add:

- `docs/compaction-policy-p3-artifact-codecs-implementation-spec.md`

Update links in:

- [compaction-policy-crate-extraction-plan.md](/home/rose/work/codex/fork/docs/compaction-policy-crate-extraction-plan.md)

### New crate

Add:

- `codex-rs/context-maintenance-policy/src/artifact_codecs.rs`
- `codex-rs/context-maintenance-policy/src/thread_memory.rs`
- `codex-rs/context-maintenance-policy/src/continuation_bridge/mod.rs`
- `codex-rs/context-maintenance-policy/src/continuation_bridge/baton.rs`
- `codex-rs/context-maintenance-policy/src/continuation_bridge/rich_review.rs`

Update:

- `codex-rs/context-maintenance-policy/src/lib.rs`
- `codex-rs/context-maintenance-policy/src/tests.rs`
- `codex-rs/context-maintenance-policy/Cargo.toml`
  - add `serde`
  - add `serde_json`

### `codex-core`

Touch minimally:

- [continuation_bridge.rs](/home/rose/work/codex/fork/codex-rs/core/src/continuation_bridge.rs)
- [governance/thread_memory.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/thread_memory.rs)
- [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)
- [compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)

The intended pattern is:

- core execution wrappers stay in `codex-core`
- artifact-family meaning moves to the policy crate

## Validation

For `p3`, run:

```bash
cd /home/rose/work/codex/fork/codex-rs
cargo test -p codex-context-maintenance-policy
cargo test -p codex-core compaction_policy_matrix_tests --lib
cargo test -p codex-core compact::tests --lib
cargo test -p codex-core context_maintenance::tests --lib
just fix -p codex-context-maintenance-policy
just fix -p codex-core
just fmt
```

Do not run a full workspace test pass solely for `p3`.

## Exit Criteria

`p3` is complete when:

1. continuation-bridge artifact logic is policy-owned
2. thread-memory artifact logic is policy-owned
3. prune-manifest and tagged-artifact codecs are policy-owned
4. `codex-core` execution wrappers call policy-owned artifact helpers rather
   than owning those semantics directly
5. current runtime behavior remains preserved by existing and migrated tests
6. timing-law activation is still deferred cleanly to `p4`

At that point the branch is ready for:

- `p4` making `MaintenanceTiming` authoritative in live routing
- or an updated draft PR if you want review before activating the timing split

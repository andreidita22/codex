# Compaction Policy P2 History-Shaping Implementation Spec

## Purpose

This spec defines `p2` of the compaction-policy extraction:

- move pure history-shaping logic out of `codex-core`
- make the new policy crate own deterministic replacement-history assembly rules
- keep runtime behavior unchanged while the ownership boundary moves

This step follows:

- [docs/compaction-policy-crate-extraction-plan.md](compaction-policy-crate-extraction-plan.md)
- [docs/compaction-policy-route-matrix.md](compaction-policy-route-matrix.md)
- [docs/compaction-policy-p0-route-matrix-implementation-spec.md](compaction-policy-p0-route-matrix-implementation-spec.md)
- [docs/compaction-policy-p1-scaffold-implementation-spec.md](compaction-policy-p1-scaffold-implementation-spec.md)

## Interpretation

`p2` is the first real extraction step.

Unlike `p1`, this phase does move live logic. But the move should remain narrow:

- extract pure history-shaping helpers
- keep model execution in `codex-core`
- keep artifact codecs and prompt builders out of scope
- preserve current runtime behavior

This is still not the timing-law activation step. `p2` should move ownership of
deterministic shaping, not redesign semantics.

## Scope

`p2` should do three things:

1. move pure history-shaping helpers into
   `codex-context-maintenance-policy`
2. make `codex-core` call those helpers through the new crate
3. preserve the current observed behavior locked by `p0`

`p2` should not:

- move continuation-bridge or thread-memory prompt/spec builders
- move tagged artifact parsing/normalization
- change the current `/refresh` overlap behavior
- activate the target `TurnBoundary` vs `IntraTurn` law
- change TUI, config, or app-server surfaces

## Why P2 Comes Before Artifact Codecs

The current code already contains a clear seam of pure shaping logic:

- retention windows
- insertion ordering
- initial-context placement
- remote compacted-history post-processing

Those rules are deterministic and structural. They are the easiest part to move
without entangling prompt semantics, artifact payload parsing, or model calls.

That makes `p2` a good extraction slice:

- big enough to matter architecturally
- small enough to keep behavior stable

## Current Code Surface To Move

### From `compact.rs`

In [compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs):

- `retain_recent_raw_conversation_messages`
- `insert_initial_context_before_last_real_user_or_summary`
- `insert_items_before_last_summary_or_compaction`

These are already pure deterministic helpers and should become policy-owned.

### From `compact_remote.rs`

In [compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs):

- `process_compacted_history(...)`

This is the most important remote-path shaping seam. It is not just glue code:
it filters remote output, re-applies the placement law, and rebuilds retained
history around authoritative items.

`p2` should move the pure shaping part of this function behind the new crate.

### Call sites to rewire

The existing live callers are:

- [compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)
- [compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs)
- [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)

`p2` should rewrite those call sites to use the extracted crate helpers while
keeping their current orchestration flow.

## Explicit Non-Scope For P2

These surfaces should stay in `codex-core` for now:

- tagged artifact detection in
  [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)
  (`has_tagged_block`, prune-manifest classification)
- prune-manifest builder logic
- continuation-bridge prompt/spec builders
- thread-memory prompt/spec builders
- `ResponseItem` <-> tagged-payload codecs
- artifact inventory extraction

Those belong to `p3`.

This also applies to prune-side supersession helpers whose correctness still
depends on artifact classification:

- prune supersession filtering
- latest-summary retention decisions that rely on tagged artifact identity
- explicit artifact-kind removal driven by tagged-block detection

Those helpers should remain in `codex-core` for `p2` because artifact
classification is still core-owned in this phase.

## Proposed Policy-Crate Module Expansion

Extend the new crate with two modules:

- `history_shape.rs`
  - insertion/placement rules
  - remote compacted-history post-processing
- `retention.rs`
  - raw-tail retention and window trimming

Suggested updated crate layout:

- `src/contracts.rs`
- `src/route_matrix.rs`
- `src/history_shape.rs`
- `src/retention.rs`
- `src/tests.rs`

Keep `p2` focused. Do not add `artifact_codecs.rs` yet.

## API Shape For P2

`p1` kept the contract planning-only. `p2` is the first point where the new
crate genuinely needs protocol-bearing inputs, but it should still stay close
to the extracted helper granularity.

Do not introduce a large final-form assembly API yet. Prefer a small set of
pure helper functions plus, if needed, one remote-specific shaping function.

If request structs are introduced, they should stay narrow and helper-shaped.
For example:

```rust
pub struct RemoteCompactedHistoryRequest {
    pub compacted_history: Vec<ResponseItem>,
    pub initial_context: Vec<ResponseItem>,
    pub authoritative_items: Vec<ResponseItem>,
    pub raw_message_retention_limit: usize,
    pub context_injection: ContextInjectionPolicy,
    pub retain_recent_raw_messages: bool,
}
```

The governing rules are:

- keep shaping inputs explicit and pure
- do not pass `Session`, `TurnContext`, or model clients into the new crate
- do not make the new crate infer semantics from opaque `ResponseItem`s when
  `p2` has explicitly deferred artifact codecs and inventory extraction

That last point matters on the remote path. If the extracted helper is expected
to reinsert authoritative items and apply a retention window while preserving
current behavior, it needs an explicit input that tells it whether the retention
step should run. It should not be asked to rediscover that from raw items in
`p2`.

## Functions To Extract In P2

### 1. Raw-tail retention

Move the logic currently in:

- `retain_recent_raw_conversation_messages(...)`

to the new `retention.rs`.

This should remain a pure helper over `Vec<ResponseItem>`.

### 2. Initial-context placement

Move the logic currently in:

- `insert_initial_context_before_last_real_user_or_summary(...)`

to `history_shape.rs`.

This keeps the context-injection law attached to policy-owned shaping.

### 3. Authoritative-item placement

Move the logic currently in:

- `insert_items_before_last_summary_or_compaction(...)`

to `history_shape.rs`.

This placement rule is shared by local compact, remote compact, and
maintenance flows, so it belongs in the policy crate.

### 4. Remote compacted-history processing

Split `process_compacted_history(...)` into:

- a thin `codex-core` orchestration wrapper
- a pure policy-crate shaping function

The extraction target here includes the remote filtering rule itself, not just
the outer wrapper function. If the current remote path uses an internal
predicate/helper to drop non-user content messages, that filtering law should
move with the extracted shaping function in `p2`.

The new pure function should own:

- dropping non-user content messages from remote compacted history
- reinserting initial context according to `ContextInjectionPolicy`
- reinserting authoritative items at the correct placement boundary
- applying the retention window

The `codex-core` wrapper should continue to own:

- session/context derivation
- artifact generation calls
- building `initial_context` and `authoritative_items`

If keeping the original name would understate the broader role after
extraction, prefer a new helper name in the policy crate that reflects full
remote history shaping rather than only one processing substep.

## Behavioral Constraints

`p2` must preserve current behavior.

That means:

- the `p0` current-state tests stay authoritative for current behavior
- the existing `compact::tests::*` around placement and remote processing must
  remain green
- `/refresh` continues to regenerate both `thread_memory` and
  `continuation_bridge` for now
- remote-vanilla refresh remains unsupported only in the target planning law,
  not via runtime adoption in this slice

## Test Plan

### New crate tests

Add direct tests in the new crate for extracted helpers:

- retention keeps recent raw messages and preserves summary/compaction tail
- insertion keeps final summary last
- insertion keeps final compaction marker last
- initial-context placement inserts before last real user or summary
- remote compacted-history shaping drops non-user content and preserves allowed
  retained items

These should be migrated or mirrored from the existing `compact::tests::*`
coverage rather than rewritten from scratch.

In `p2`, these are **current-behavior-preservation tests**, not target-law
activation tests. The policy crate will temporarily contain:

- target route semantics from `p1`
- current shaping semantics preserved during extraction in `p2`

Those layers should stay explicit and should not be blended into one combined
test story.

### `codex-core` guardrails

Keep these green:

- `cargo test -p codex-core compaction_policy_matrix_tests --lib`
- `cargo test -p codex-core compact::tests --lib`
- `cargo test -p codex-core context_maintenance::tests --lib`

The goal is to prove the extraction preserved behavior.

## Proposed File Changes

### Docs

Add:

- `docs/compaction-policy-p2-history-shaping-implementation-spec.md`

Update links in:

- [compaction-policy-crate-extraction-plan.md](/home/rose/work/codex/fork/docs/compaction-policy-crate-extraction-plan.md)

### New crate

Add:

- `codex-rs/context-maintenance-policy/src/history_shape.rs`
- `codex-rs/context-maintenance-policy/src/retention.rs`

Update:

- `codex-rs/context-maintenance-policy/src/lib.rs`
- `codex-rs/context-maintenance-policy/src/tests.rs`
- `codex-rs/context-maintenance-policy/Cargo.toml`
  - add `codex-protocol`

### `codex-core`

Touch minimally:

- [compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)
- [compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs)
- [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)

The intended pattern is:

- core orchestration stays in `codex-core`
- deterministic shaping calls move to the new crate

## Validation

For `p2`, run:

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

Do not run a full workspace test pass solely for `p2`.

## Exit Criteria

`p2` is complete when:

1. the pure history-shaping helpers live in `codex-context-maintenance-policy`
2. `codex-core` calls those helpers instead of owning the logic directly
3. remote compacted-history shaping is routed through the new crate
4. current behavior remains preserved by existing and migrated tests
5. artifact codecs and prompt builders are still deferred cleanly to `p3`

At that point the branch is ready for:

- `p3` artifact codecs and prompt/spec extraction
- or an updated draft PR if you want review before moving the artifact layer

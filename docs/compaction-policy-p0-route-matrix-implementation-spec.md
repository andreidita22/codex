# Compaction Policy P0 Implementation Spec

## Purpose

This spec defines the first implementation step for the compaction-policy
extraction:

- lock the route matrix in code-facing artifacts
- add fast fixture-style tests against the current implementation
- avoid semantic drift before any crate extraction begins

This is `p0` from:

- [docs/compaction-policy-crate-extraction-plan.md](compaction-policy-crate-extraction-plan.md)
- [docs/compaction-policy-route-matrix.md](compaction-policy-route-matrix.md)

## Interpretation

`p0` is a **current-state lock**, not yet full convergence to the target route
matrix.

That means `p0` must distinguish between:

- **target semantics**
  - the authoritative future law in
    [compaction-policy-route-matrix.md](/home/rose/work/codex/fork/docs/compaction-policy-route-matrix.md)
- **current observed behavior**
  - what the current fork actually does today and therefore must be frozen in
    tests before extraction

Where those differ, `p0` should record the difference explicitly as a
known delta rather than silently treating both as equally authoritative.

## Scope

This step is intentionally narrow.

It does **not**:

- create `codex-context-maintenance-policy`
- move logic out of `codex-core`
- change runtime behavior intentionally
- introduce new public config surface

It **does**:

- freeze the current intended semantics as executable fixtures/tests
- make the route matrix a code-checked artifact
- establish the exact seams that later PRs will extract

## Why P0 Comes First

Right now the semantic split already exists implicitly in `codex-core`, but it
is encoded through scattered conditionals and path-specific behavior:

- `InitialContextInjection::{DoNotInject, BeforeLastUserMessage}`
  in [compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)
- `should_regenerate_thread_memory(...)`
  in [compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)
- separate local/remote history-shaping paths in
  [compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs) and
  [compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs)
- between-turn maintenance behavior in
  [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)

If we start moving code before locking those semantics, the extraction will
accidentally become a redesign.

## Current Code Surface To Lock

### Compaction helpers already encoding hidden law

In [compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs):

- `InitialContextInjection`
- `should_use_remote_compact_task`
- `should_regenerate_thread_memory`
- `retain_recent_raw_conversation_messages`
- `insert_initial_context_before_last_real_user_or_summary`
- `insert_items_before_last_summary_or_compaction`

These are the current pure-ish helpers that the policy crate will later absorb.

### Remote-path reuse of local helpers

In [compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs):

- remote compaction already depends on the same history-shaping helpers from
  `compact.rs`
- this confirms the shaping law is shared and should become policy-owned

### Between-turn maintenance logic

In [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs):

- `/refresh` currently regenerates both `thread_memory` and
  `continuation_bridge`
- `/prune` emits prune manifests and uses the same retention helpers

This file is one of the main places where the future route matrix will force
behavioral clarification.

### Remote output post-processing

In [compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs):

- `process_compacted_history(...)`

This is part of the semantic seam too. The remote path does not only reuse local
helpers; it also filters remote output and then applies the same placement law.
That makes it part of the route-matrix surface that should be locked before
extraction.

### Existing tests already close to fixture form

In [compact_tests.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_tests.rs):

- tests already cover:
  - summary placement
  - initial-context insertion
  - compaction-marker-last placement
  - retention window behavior

Those tests should be retained and extended rather than replaced.

## Deliverables

P0 should produce four concrete artifacts.

### 1. A checked route matrix doc

The existing route-table doc:

- [compaction-policy-route-matrix.md](/home/rose/work/codex/fork/docs/compaction-policy-route-matrix.md)

becomes the authoritative semantic source for extraction.

No additional doctrine expansion is needed in P0 beyond keeping it synchronized
with the tests below.

### 2. A compact test-only route model inside `codex-core`

Add a small test-only typed model that mirrors the matrix, for example in a new
test module/file:

- `codex-rs/core/src/compaction_policy_matrix_tests.rs`

This model should be test-only, not yet production API.

Suggested shape:

```rust
enum ExpectedAction {
    Compact,
    Refresh,
    Prune,
}

enum ExpectedTiming {
    TurnBoundary,
    IntraTurn,
}

enum ExpectedEngine {
    RemoteVanilla,
    RemoteHybrid,
    LocalPure,
}

struct ExpectedRoute {
    action: ExpectedAction,
    timing: ExpectedTiming,
    engine: ExpectedEngine,
    generates_thread_memory: bool,
    generates_continuation_bridge: bool,
    context_injection: ExpectedContextInjection,
    preserves_legacy_compaction_marker: bool,
    result_kind: ExpectedRouteResult,
}
```

This is not the future policy-crate API. It is a fixture lock.

### 3. Fixture-style tests against current helpers

Add table-driven tests that assert the current implementation matches the route
matrix at the pure-helper level.

At minimum, cover:

- `Compact + IntraTurn`
  - maps to `InitialContextInjection::BeforeLastUserMessage`
  - suppresses durable `thread_memory` regeneration
- `Compact + TurnBoundary`
  - maps to `InitialContextInjection::DoNotInject`
  - allows durable `thread_memory` regeneration
- `Refresh + TurnBoundary`
  - currently regenerates both `thread_memory` and `continuation_bridge`
  - this should be asserted as current observed behavior and marked as a known
    delta against the target matrix
- `Prune + TurnBoundary`
  - emits prune manifest
  - does not generate semantic artifacts
- invalid routes
  - `Refresh + IntraTurn`
  - `Prune + IntraTurn`
  - should be represented explicitly in the fixture model even if current code
    does not yet have first-class enforcement

### 4. Explicit extraction target list

Record, in the P0 spec or the test module header, the exact pure helpers that
later PRs should move first:

- `retain_recent_raw_conversation_messages`
- `insert_initial_context_before_last_real_user_or_summary`
- `insert_items_before_last_summary_or_compaction`
- `process_compacted_history(...)` in
  [compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs)
- artifact-pruning helpers in
  [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)

This keeps later extraction scoped.

## Proposed File Changes

### Docs

- no new conceptual doctrine docs required
- optionally link this spec from:
  - [compaction-policy-crate-extraction-plan.md](/home/rose/work/codex/fork/docs/compaction-policy-crate-extraction-plan.md)

### Rust

Add:

- `codex-rs/core/src/compaction_policy_matrix_tests.rs`

Touch minimally:

- [lib.rs](/home/rose/work/codex/fork/codex-rs/core/src/lib.rs)
  - include the new test module
- [compact_tests.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_tests.rs)
  - only if small helper reuse is cleaner than duplicating fixture scaffolding

Avoid widening changes in:

- [compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)
- [compact_remote.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact_remote.rs)
- [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)

unless a tiny visibility adjustment is required for test access.

Prune/refresh helper assertions should stay close to
[context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)
if that avoids broadening visibility. The new root-level matrix test module
should focus on:

- route-model fixtures
- `compact.rs` helper behavior
- shared shaping laws

and leave prune-manifest/private maintenance-helper checks in the existing
context-maintenance test surface.

## Test Design

### Fast unit focus

P0 should prefer fast unit tests over integration tests.

Why:

- the goal is semantic freeze, not runtime end-to-end validation
- we want low rebuild cost before the crate split exists

### Assertions to add

At minimum:

1. Timing-to-initial-context mapping
   - `IntraTurn` implies `BeforeLastUserMessage`
   - `TurnBoundary` implies `DoNotInject`

2. Timing-to-thread-memory regeneration mapping
   - `IntraTurn` suppresses durable memory regeneration
   - `TurnBoundary` permits it

Because the current code does not yet expose these as first-class policy types,
`p0` may introduce one tiny internal classifier/helper purely for testability if
needed. That is acceptable as long as it does not widen runtime behavior or
pretend the extraction has already happened.

3. Placement laws
   - initial context inserted before last real user or summary
   - authoritative artifacts inserted before summary or compaction marker
   - compaction marker stays last when present

4. Retention laws
   - raw tail window preserves recent messages
   - summary/marker survive retention as expected

5. Current overlap law
   - `/refresh` currently regenerates both `thread_memory` and bridge
   - this is intentionally asserted as current behavior so later PRs can change
     it deliberately with fixture updates

6. Prune law
   - prune manifest is explicit
   - stale manifests are superseded

### Invalid-route treatment

Even if the current production code does not yet expose a first-class
`MaintenanceTiming` type, the test fixture model should include unsupported
routes and mark them invalid:

- `Refresh + IntraTurn`
- `Prune + IntraTurn`

This is important because it prevents later code motion from silently inventing
fallback semantics.

## Known Deltas Against Target Matrix

These deltas should be named explicitly in `p0` tests/comments so later PRs can
remove them deliberately.

### Delta 1: `/refresh`

Target matrix:

- `Refresh + TurnBoundary` should regenerate durable `thread_memory`
- it should **not** generate `continuation_bridge`

Current observed behavior:

- `/refresh` currently regenerates both `thread_memory` and
  `continuation_bridge`

P0 requirement:

- assert the current overlap
- label it as a known delta against target semantics

No other route should silently mix current-state locking and target semantics.

## Test Commands

Targeted validation for P0:

```bash
cd /home/rose/work/codex/fork/codex-rs
cargo test -p codex-core compaction_policy_matrix_tests --lib
cargo test -p codex-core --lib compact::
cargo test -p codex-core --lib context_maintenance
just fmt
```

Use module-path filters rather than filename-style guesses. The exact filter may
need minor adjustment to match the final test module names.

## Non-Goals

P0 should not:

- introduce the new workspace crate
- change config parsing
- change TUI behavior
- switch live `/refresh` semantics yet
- remove legacy `ResponseItem::Compaction` markers yet
- make local-pure vs hybrid behavior diverge more than it already does

## Definition Of Done

P0 is done when:

1. the route matrix exists as an authoritative doc
2. the current implementation is checked against that matrix with fast tests
3. unsupported routes are explicitly represented as invalid in fixtures
4. the exact pure helpers to extract in P1/P2 are named and covered
5. later crate extraction can proceed without rediscovering semantics from code

## Recommended Commit Shape

Single PR:

- docs
  - route-matrix/spec link if needed
- tests
  - new fixture-style route matrix tests in `codex-core`
- tiny visibility adjustments only if required for test access

This should remain a low-risk, semantics-locking PR rather than a behavior PR.

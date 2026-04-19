## 1. Current-state assessment

**Observed:** the fork is materially cleaner than the earlier state. The policy crate is now a real boundary, not just a folder move.

Strong points in the current source:

* `context-maintenance-policy` no longer depends on `codex-config` or `codex-core`; its only Codex crate dependency is `codex-protocol`.
* Route law is mostly centralized in `context-maintenance-policy/src/route_matrix.rs`.
* Policy-owned typed concepts now exist for action, timing, engine, artifact kind, artifact lifetime, artifact requiredness, retention, legacy marker policy, and thread-memory governance.
* `core/src/context_maintenance_runtime.rs` is a mostly legitimate adapter from runtime/config concepts into policy concepts.
* Artifact requiredness now survives into core execution via `ArtifactRequest` and `execute_requested_artifact`.
* Continuation bridge prompts, schemas, payload parsing, response-item construction, and subagent supplemental block construction are policy-owned.
* Thread-memory prompt, schema, parsing, tagged response-item construction, and much of source trimming are policy-owned.
* TUI/app-server/protocol crates do not depend on the policy crate. They expose operations/config, not policy law.

**Executive judgment:** the boundary is fundamentally correct, but not maximally isolated. The remaining weaknesses are not large redesign problems; they are residual law surfaces around source selection, history disposition, generic protocol utilities, public API breadth, and tests that still duplicate doctrine.

The main thing still preventing maximal isolation from vanilla is that `core` still decides several maintenance semantics at execution time instead of consuming a fully specified policy plan. The largest remaining risks are in `core/src/compact.rs`, `core/src/compact_remote.rs`, and `core/src/context_maintenance.rs`.

---

## 2. Observed boundary map

### Good contact points

`context-maintenance-policy/Cargo.toml`

* Depends on `codex-protocol`, `serde`, `serde_json`, `thiserror`, and `tracing`.
* No direct dependency on `codex-core`, `codex-config`, TUI, app-server, runtime services, or session state.
* This is the right crate-level dependency shape.

`core/src/context_maintenance_runtime.rs`

* Maps `CompactionEngine` to `PolicyEngine`.
* Maps core `GovernancePathVariant` to policy `ThreadMemoryGovernance`.
* Maps compact invocation timing to policy `MaintenanceTiming`.
* Preserves `ArtifactRequest` rather than collapsing it to `ArtifactKind`.
* Applies plan-driven required/best-effort generation behavior.

This is an acceptable adapter.

`core/src/continuation_bridge.rs` and `core/src/continuation_bridge/subagent_context.rs`

* Core owns model streaming, session lookup, request context, and runtime subagent inventory.
* Policy owns prompt construction, schema, parser, response item, and subagent supplemental tag/shape.
* `subagent_context.rs` is now a narrow runtime-to-policy translation.

This is a good boundary.

`core/src/governance/thread_memory.rs`

* Core owns model streaming and request context.
* Policy owns the model-facing prompt/schema/parser/tagged artifact.
* This is mostly correct, with remaining source-selection issues noted below.

### Risky contact points

`core/src/compact.rs`

* Still contains local compact history assembly, source history passed to artifact generators, summary detection, raw-message classifiers, and executor selection.
* Some of this is runtime/upstream-shaped and acceptable.
* The risky part is that artifact generation runs before policy disposition is applied, so stale policy artifacts can still influence new artifacts.

`core/src/compact_remote.rs`

* Uses policy history shaping, but still owns the full remote compact output keep/drop predicate in `should_keep_compacted_history_item`.
* That predicate is policy-shaped law, not just runtime translation.

`core/src/context_maintenance.rs`

* `/refresh` and `/prune` now use `apply_history_disposition`, but core still chooses `prune_superseded_artifacts = true`.
* Core still owns prune summary retention via `retain_latest_summary_message`.
* Core still decides prune-manifest stats passed into the policy-owned manifest builder.

`core/src/lib.rs`

* Re-exports `codex_context_maintenance_policy::content_items_to_text`.
* That turns the policy crate into a general utility provider for unrelated core modules.

---

## 3. Prioritized findings

### Finding 1 — Artifact source selection is still not policy-complete

**Type:** boundary leak with concrete correctness risk.

**Files involved:**

* `context-maintenance-policy/src/contracts.rs:29-55`
* `context-maintenance-policy/src/route_matrix.rs:45-56`, `58-70`, `81-95`, `164-177`
* `context-maintenance-policy/src/thread_memory.rs:36-66`, `164-182`
* `context-maintenance-policy/src/continuation_bridge/mod.rs:42-61`
* `core/src/compact.rs:172-199`
* `core/src/compact_remote.rs:165-202`
* `core/src/continuation_bridge.rs:62-82`
* `core/src/governance/thread_memory.rs:28-46`

**Observed:** route plans declare prior artifact drops and artifact lifetimes, but artifact generation receives pre-disposition prompt history. Final replacement history may drop prior `ContinuationBridge`, `ThreadMemory`, or `PruneManifest`, but those prior artifacts may already have been visible to the next maintenance model call.

Thread memory has partial source filtering: after a previous thread-memory artifact is found, `split_previous_memory_and_source_items` filters later source items through `should_include_delta_source_item`. But when no prior thread memory exists, it returns `input.to_vec()` without applying the same artifact/synthetic filtering. That means first thread-memory generation can ingest stale continuation bridges, prune manifests, compaction markers, or other policy artifacts if they exist in prompt history.

Continuation bridge has no equivalent policy-owned source filter. `generate_continuation_bridge_item` passes the prompt history directly into `build_continuation_bridge_prompt_input`.

**Why it matters:** `ArtifactLifetime::TurnScoped` is currently executable only as final-history drop behavior, not as model-input source behavior. A turn-scoped continuation bridge can still influence generation of a later bridge or first thread memory. That reintroduces hidden law: the route matrix says “drop prior bridge,” but the runtime still lets prior bridge content affect new artifacts before the drop happens.

**Smallest reasonable fix:**

Add policy-owned artifact source selection, without building a generic framework.

A minimal shape would be:

```rust
pub fn select_thread_memory_source(
    input: &[ResponseItem],
    is_compaction_summary: impl Fn(&ResponseItem) -> bool,
) -> ThreadMemorySourceSelection

pub fn select_continuation_bridge_source(
    input: Vec<ResponseItem>,
) -> Vec<ResponseItem>
```

For thread memory:

* always exclude tagged context-maintenance artifacts from source items;
* always exclude `GhostSnapshot`;
* decide explicitly whether legacy compaction summaries are allowed when there is no previous thread memory;
* keep previous-memory extraction as policy-owned.

For continuation bridge:

* at minimum exclude prior `ContinuationBridge`;
* probably exclude `PruneManifest`;
* decide explicitly whether durable `ThreadMemory` is allowed as source.

Then core should call those helpers before streaming the artifact model. Add tests for:

* first thread-memory generation after a prior bridge;
* first thread-memory generation after a prune manifest;
* continuation bridge generation after a prior bridge;
* thread-memory generation with no previous memory but with a compaction marker.

**Implementation risk:** medium. It changes model input, not only deterministic post-processing. But it directly hardens the lifetime semantics already declared by the policy crate.

---

### Finding 2 — History disposition is still partially caller-selected in core

**Type:** boundary/ownership leak.

**Files involved:**

* `context-maintenance-policy/src/contracts.rs:64-70`, `112-119`
* `context-maintenance-policy/src/artifact_codecs.rs:202-243`
* `context-maintenance-policy/src/history_shape.rs:91-144`
* `core/src/context_maintenance_runtime.rs:63-74`
* `core/src/context_maintenance.rs:54-57`, `107-116`, `126-129`, `148-184`
* `core/src/compact_remote.rs:227-243`, `267-299`, `301-340`

**Observed:** `MaintenancePolicyPlan` owns `drop_prior_artifact_kinds`, marker policy, and retention directive, but it does not own whether superseded artifacts should be pruned. That decision is passed as a raw bool by core:

```rust
history_disposition_request(raw_history, /*prune_superseded_artifacts*/ true)
```

Remote shaping hardcodes the opposite:

```rust
prune_superseded_artifacts: false
```

Prune summary retention is also core-local:

```rust
retain_latest_summary_message
is_compaction_summary_message
```

Remote compact output filtering is also core-local through `should_keep_compacted_history_item`.

**Why it matters:** this creates a second history law surface. The policy plan appears to own carry/drop behavior, but core still decides:

* whether superseded tagged artifacts are pruned;
* whether old compact summaries are collapsed;
* what remote compact output item classes are retained;
* what counts are fed into the prune manifest.

Future artifact families or summary forms would still reopen `core/src/context_maintenance.rs` and `core/src/compact_remote.rs`.

**Smallest reasonable fix:**

Add a policy-owned disposition directive to `MaintenancePolicyPlan`, for example:

```rust
pub struct HistoryDispositionPolicy {
    pub prune_superseded_artifacts: bool,
    pub drop_prior_artifact_kinds: Vec<ArtifactKind>,
    pub legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy,
    pub summary_disposition: SummaryDisposition,
}
```

Then remove the caller-supplied bool from `RuntimeMaintenancePlan::history_disposition_request`.

For summary retention, do not move `event_mapping` into policy. Keep core as a classifier adapter:

```rust
apply_summary_disposition(items, SummaryDisposition::KeepLatest, is_compaction_summary)
```

For remote compact output, replace the unconstrained `should_keep_item` closure with narrower predicates/classifiers, so policy owns the default remote-output retention law and core only answers runtime classification questions such as “is this a real user or hook prompt?”

**Implementation risk:** medium. This is deterministic behavior, but snapshot tests will likely move.

---

### Finding 3 — `content_items_to_text` is owned by the wrong crate now

**Type:** boundary leak.

**Files involved:**

* `context-maintenance-policy/src/artifact_codecs.rs:16-33`
* `context-maintenance-policy/src/lib.rs:11`
* `core/src/lib.rs:197`
* `core/src/arc_monitor.rs`
* `core/src/realtime_context.rs`
* `core/src/guardian/prompt.rs`
* `core/src/memories/phase1.rs`
* `core/src/continuation_bridge.rs:13`, `112-118`
* `core/src/governance/thread_memory.rs:7`, `89-95`
* `protocol/src/models.rs:1141`

**Observed:** the helper is implemented in `context-maintenance-policy`, then re-exported from `codex-core`, and then used by unrelated core modules such as ARC monitor, realtime context, guardian prompt construction, and memories.

That means a context-maintenance policy crate is now the owner of a general `ContentItem` text-extraction utility.

**Why it matters:** this broadens the policy crate into a generic utility dependency. It also makes unrelated upstream-shaped core files indirectly rely on context-maintenance code. The original PR6 cleanup intent was to single-source the helper; the actual resulting owner is semantically wrong given the widened caller set.

**Smallest reasonable fix:**

Move the helper to `codex-protocol::models`, next to `ContentItem` and the existing `function_call_output_content_items_to_text`.

Then:

* policy imports `codex_protocol::models::content_items_to_text`;
* core imports the protocol helper directly;
* remove `pub use artifact_codecs::content_items_to_text` from policy;
* remove `pub use codex_context_maintenance_policy::content_items_to_text` from `core/src/lib.rs`.

**Implementation risk:** low. This touches a vanilla-shaped crate, but it removes a worse cross-cutting policy dependency.

---

### Finding 4 — Thread-memory trimming owns protocol call/output pairing law

**Type:** boundary leak with future correctness risk.

**Files involved:**

* `context-maintenance-policy/src/thread_memory.rs:68-91`
* `context-maintenance-policy/src/thread_memory.rs:219-301`
* `protocol/src/models.rs`

**Observed:** `remove_corresponding_for` in the policy crate knows how protocol response items pair:

* `FunctionCall` ↔ `FunctionCallOutput`
* `ToolSearchCall` ↔ `ToolSearchOutput`
* `CustomToolCall` ↔ `CustomToolCallOutput`
* `LocalShellCall` ↔ `FunctionCallOutput`

This is not context-maintenance law. It is `ResponseItem` pairing law.

**Why it matters:** if upstream adds or changes a call/output pair, the policy crate becomes one of the places that must be updated. That is exactly the kind of hidden upstream-ingest seam this crate extraction is supposed to reduce.

**Smallest reasonable fix:**

Move pair relation helpers into `codex-protocol::models`, for example:

```rust
pub fn response_items_are_call_pair(a: &ResponseItem, b: &ResponseItem) -> bool
```

or a small call-id classification helper.

Then keep thread-memory trimming policy-owned, but delegate pair identity to protocol.

**Implementation risk:** medium-low. It is deterministic and already covered by policy tests, but it touches response-item semantics.

---

### Finding 5 — Governance-off behavior is under-specified for existing durable thread memory

**Type:** boundary leak; possible correctness issue depending intended semantics.

**Files involved:**

* `context-maintenance-policy/src/contracts.rs:78-102`
* `context-maintenance-policy/src/route_matrix.rs:186-208`
* `context-maintenance-policy/src/route_matrix.rs:164-177`
* `context-maintenance-policy/src/retention.rs:41-85`
* `core/src/compaction_policy_matrix_tests.rs:85-108`, `169-211`

**Observed:** `ThreadMemoryGovernance::Disabled` removes requested `ThreadMemory` artifacts only when the base route requested one. It does not explicitly say what happens to existing durable thread-memory artifacts. It also leaves retention directives unchanged.

For turn-boundary compact/refresh, base routes already drop prior thread memory, so the practical effect is mostly safe.

For prune, the base route requests only `PruneManifest` and drops `PruneManifest` plus `ContinuationBridge`, not `ThreadMemory`. The retention directive is still gated on final history containing `ThreadMemory`. So governance-off plus an existing durable thread-memory artifact can still trigger thread-memory-based raw-history retention during prune.

**Why it matters:** the enum name `Disabled` reads like “thread-memory law is off,” but the implementation is closer to “do not request a new thread-memory artifact.” Those are different policies. Current tests even lock retention remaining active when governance is off.

**Smallest reasonable fix:**

Make the semantics explicit in the policy type. Either:

```rust
ThreadMemoryGovernance::DoNotGenerateButHonorExisting
ThreadMemoryGovernance::DisabledDropAndIgnore
```

or keep the existing enum but change the disabled overlay to:

* add `ArtifactKind::ThreadMemory` to drops for every route;
* set retention to `RetentionDirective::None` when retention is only thread-memory-gated;
* emit a governance effect for “existing thread memory ignored/dropped.”

Add a direct route/history test for prune with existing thread memory and governance off.

**Implementation risk:** medium if product semantics intend “honor existing.” Low if the intent is “disabled means disabled.”

---

### Finding 6 — Policy public API remains wider than needed

**Type:** boundary leak / optional cleanup, with one semantic footgun.

**Files involved:**

* `context-maintenance-policy/src/lib.rs:9-58`
* `context-maintenance-policy/src/contracts.rs:42-48`
* `context-maintenance-policy/src/artifact_codecs.rs:117-134`, `136-175`, `202-243`
* `context-maintenance-policy/src/continuation_bridge/mod.rs:23-27`, `140-216`
* `context-maintenance-policy/src/thread_memory.rs:19-27`, `36-91`, `112-162`

**Observed:** the policy crate publicly exports low-level helpers and intermediate model payload types that core does not need to understand as independent concepts.

Examples:

* `prune_superseded_artifacts`
* `remove_artifact_kind`
* `retain_recent_raw_conversation_messages`
* `ContinuationBridgePayload`
* `ThreadMemorySourceSelection`
* `ThreadMemoryTrimResult`
* split parse/build pairs for model outputs

The sharpest footgun is `ArtifactKind::CompactionMarker`. It is a public artifact kind, but `remove_artifact_kind` removes only tagged developer artifacts. Legacy `ResponseItem::Compaction` stripping happens through `LegacyCompactionMarkerPolicy` inside `apply_history_disposition`, not through `ArtifactKind::CompactionMarker`.

A caller can reasonably infer that `remove_artifact_kind(items, ArtifactKind::CompactionMarker)` removes compaction markers. It does not.

**Why it matters:** wide public exports make it easier for core tests or future runtime code to build a second law surface around low-level helpers. The `CompactionMarker` mismatch is specifically misleading API.

**Smallest reasonable fix:**

* Hide `prune_superseded_artifacts`, `remove_artifact_kind`, and raw retention helpers unless production core truly needs them.
* Move tests that exercise those helpers into the policy crate.
* Replace parse-then-response-item public pairs with direct artifact functions where core only needs a `ResponseItem`, for example:

```rust
thread_memory_response_item_from_model_output(result: &str) -> CodexResult<Option<ResponseItem>>
continuation_bridge_response_item_from_model_output(
    result: &str,
    fallback_variant: BridgeVariant,
) -> CodexResult<Option<ResponseItem>>
```

* Remove `ArtifactKind::CompactionMarker` from public artifact-removal paths, or make all public APIs that accept it handle legacy `ResponseItem::Compaction` correctly.

**Implementation risk:** low to medium. Mostly API narrowing and test relocation.

---

### Finding 7 — Artifact lifetime is declared but not independently enforced

**Type:** boundary leak / future-expansion risk.

**Files involved:**

* `context-maintenance-policy/src/contracts.rs:29-55`
* `context-maintenance-policy/src/route_matrix.rs:45-177`
* `core/src/context_maintenance_runtime.rs:37-100`
* `core/src/compact.rs:314-339`
* `core/src/compact_remote.rs:227-243`
* `core/src/context_maintenance.rs:54-73`, `107-129`

**Observed:** `ArtifactLifetime` is preserved in `ArtifactRequest`, but runtime execution mostly uses only `ArtifactRequiredness`. Lifetime currently affects behavior indirectly through route-specific `drop_prior_artifact_kinds`, not through an invariant or executor rule.

For current artifacts this is mostly okay because route matrix entries explicitly drop prior bridges/thread memory where needed. But future artifact families can declare `TurnScoped` without the executor enforcing or validating the corresponding drop/source-selection behavior.

**Why it matters:** the type looks stronger than it is. It documents lifetime but does not protect lifetime. This is how future hidden law surfaces reappear.

**Smallest reasonable fix:**

Do not build a lifetime framework. Add route-plan invariants in policy tests:

* every `TurnScoped` artifact kind must be dropped on routes where a prior instance could be visible;
* `DurableAcrossTurns` artifacts must be explicitly either preserved, refreshed, or dropped by route law;
* `MarkerOnly` artifacts must not be used as retention gates unless explicitly intended.

Then tie source-selection helpers from Finding 1 to lifetime where practical: a prior `TurnScoped` artifact should not be visible to generation of the next artifact unless the route explicitly allows it.

**Implementation risk:** low if tests only; medium if source selection behavior changes.

---

### Finding 8 — Thread-memory source selection remains stringly at the summary boundary

**Type:** boundary leak / optional cleanup.

**Files involved:**

* `context-maintenance-policy/src/thread_memory.rs:36-39`, `164-182`
* `core/src/governance/thread_memory.rs:28-29`
* `core/src/compact.rs:53-54`, `493-495`

**Observed:** core passes `SUMMARY_PREFIX.trim_end()` into the policy crate. The policy crate then detects compact summaries by string prefix.

**Why it matters:** the policy crate owns thread-memory source selection, but one of its exclusions depends on a core template string. If summary marker format changes, both compact summary generation and thread-memory source filtering must stay aligned manually.

**Smallest reasonable fix:**

Replace `summary_prefix: &str` with a classifier callback:

```rust
split_previous_memory_and_source_items(
    input,
    is_compaction_summary_message,
)
```

or introduce a tiny policy-facing `SourceItemClassifier` that core implements using `event_mapping`/summary prefix.

This keeps the compact summary template in core while removing raw-string dependence from policy.

**Implementation risk:** low.

---

### Finding 9 — Unsupported-route error wrapping duplicates the message and tests lock it

**Type:** optional cleanup / test doctrine smell.

**Files involved:**

* `context-maintenance-policy/src/contracts.rs:122-131`
* `core/src/context_maintenance_runtime.rs:255-257`
* `core/src/compaction_policy_matrix_tests.rs:154-166`

**Observed:** policy error display already says:

```text
unsupported context-maintenance route: ...
```

Core wraps it with:

```text
Unsupported context-maintenance route: {err}
```

The resulting error is:

```text
Unsupported context-maintenance route: unsupported context-maintenance route: ...
```

A core test asserts that duplicated wording.

**Why it matters:** low severity, but it shows adapter tests are starting to lock accidental presentation details rather than boundary behavior.

**Smallest reasonable fix:**

Change core to use `err.to_string()` directly or change the policy error text to omit the prefix. Update the test to assert route failure and the route fields, not duplicated prose.

**Implementation risk:** low.

---

### Finding 10 — Test layout still creates duplicate doctrine

**Type:** optional cleanup, but important for boundary integrity.

**Files involved:**

* `context-maintenance-policy/src/tests.rs`
* `core/src/compaction_policy_matrix_tests.rs`
* `core/src/context_maintenance.rs:196-281`
* `core/src/compact_tests.rs`
* `core/src/codex_tests_guardian.rs`

**Observed:** policy crate tests now cover route doctrine and deterministic helpers. But core tests still mirror significant parts of the route matrix and exercise low-level policy helpers directly.

Examples:

* `core/src/compaction_policy_matrix_tests.rs` mirrors route-matrix assertions almost line-for-line.
* `core/src/context_maintenance.rs` tests call `prune_superseded_artifacts` and `remove_artifact_kind` directly, even though production now uses `apply_history_disposition`.
* `core/src/compact_tests.rs` contains tests for generic `content_items_to_text` and policy-owned retention/insertion behavior through core wrappers.

**Why it matters:** this risks creating two sources of doctrine: one in policy tests and one in core tests. It also keeps low-level policy exports public just to satisfy core tests.

**Smallest reasonable fix:**

* Keep route-law tests in `context-maintenance-policy`.
* Keep core matrix tests only for adapter mapping: config engine → policy engine, governance variant → policy governance, timing → policy timing, and fail-closed propagation.
* Move low-level artifact/disposition/retention tests into the policy crate.
* Convert core `/refresh` and `/prune` tests into production-flow tests that call the same high-level helpers as runtime, not old staged helper sequences.

**Implementation risk:** low.

---

## 4. Changes to avoid

Do not move `Session`, `TurnContext`, model-client streaming, telemetry, task spawning, or event emission into `context-maintenance-policy`. Those are runtime concerns.

Do not make TUI, app-server, or protocol operation handling depend on the policy crate. The current TUI/app-server/protocol contact is acceptable because it exposes user operations and config, not policy law.

Do not introduce a plugin registry or generalized artifact framework yet. The current enum-and-plan model is adequate. The problem is incomplete execution of the existing plan, not lack of abstraction.

Do not mirror `ResponseItem` into a separate policy AST. That would create a second protocol law surface. Use `codex-protocol` directly where artifacts are physically `ResponseItem`s, and move protocol-generic helpers into `codex-protocol` when needed.

Do not merge durable state DB memory mode with context-maintenance `ThreadMemory` artifact semantics. They are adjacent but not the same boundary.

Do not move vanilla compact summary generation wholesale into policy. Only extract the parts that are actually context-maintenance law: source selection, disposition, insertion, retention, and artifact ownership.

---

## 5. Recommended follow-up sequence

### PR 1 — Make artifact source selection policy-owned

* Add tests for first thread-memory generation with prior bridge/prune artifacts.
* Filter thread-memory source items consistently even when no previous thread memory exists.
* Add continuation-bridge source filtering for prior continuation bridges.
* Keep runtime model streaming in core.

This addresses the highest-risk semantic leak: stale turn-scoped artifacts influencing new artifacts before final-history disposition.

### PR 2 — Move full history disposition into the policy plan

* Add plan-owned `HistoryDispositionPolicy`.
* Remove caller-supplied `prune_superseded_artifacts: bool`.
* Move prune summary retention behind a policy directive plus core classifier.
* Narrow remote compact output filtering so policy owns the shape and core supplies classification predicates.

This closes the remaining carry/drop law split.

### PR 3 — Move `content_items_to_text` to `codex-protocol`

* Add generic `content_items_to_text` near `ContentItem`.
* Update policy and core callers.
* Remove the policy export and core re-export.

This prevents the policy crate from becoming a general utility dependency.

### PR 4 — Move response-item call/output pairing into protocol

* Add a small protocol helper for call/output pair detection.
* Update thread-memory trimming to use it.
* Keep trimming limits and source-selection policy in the policy crate.

This reduces protocol-variant churn inside the policy crate.

### PR 5 — Clarify governance-off durable-memory behavior

* Decide whether governance off means “do not generate” or “drop/ignore existing.”
* Encode that distinction in policy types.
* Add a prune test with existing thread memory and governance off.

This removes an ambiguous governance overlay.

### PR 6 — Narrow policy public API and relocate tests

* Hide low-level artifact helpers not needed by production core.
* Remove or fix the `ArtifactKind::CompactionMarker` public footgun.
* Merge parse/build pairs where core only needs a `ResponseItem`.
* Move deterministic helper tests from core into policy.
* Reduce core matrix tests to adapter behavior.

Net result: the module remains small and concrete, but the remaining policy law stops leaking through hot upstream-shaped files.
